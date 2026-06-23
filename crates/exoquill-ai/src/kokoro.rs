//! Native Kokoro-82M text-to-speech provider (hexgrad/Kokoro), via ONNX Runtime.
//!
//! Kokoro is a lightweight (82 M parameter) StyleTTS2-based TTS model that
//! outputs 24 kHz mono PCM. This provider runs Kokoro ONNX models directly
//! through the `ort` crate (the same `load-dynamic` ONNX Runtime the Silero VAD
//! uses) — there is **no Python sidecar**.
//!
//! ## Multi-language engines
//!
//! A single [`KokoroTts`] can hold several **engines**, each its own ONNX model
//! plus voices and a phonemization *dialect*. We ship two:
//!
//! - **English** — `onnx-community/Kokoro-82M-v1.0-ONNX`, phonemized with espeak
//!   `en-us` and the misaki American-English remap (`espeak_ipa_to_misaki`).
//! - **German** — `Godelaune/Kokoro-82M-ONNX-German-Martin` (the single-speaker
//!   "Martin" voice, StyleTTS2 fine-tune), phonemized with espeak `de` and the
//!   *generic* misaki `EspeakG2P` remap (`espeak_ipa_to_misaki_generic`). The
//!   German model targets the `thewh1teagle/kokoro-onnx` runtime, which uses a
//!   raw-espeak-IPA pipeline (stress marks kept, **no** English FROM_ESPEAKS
//!   remap) filtered against the shared Kokoro vocab.
//!
//! Both dialects share the canonical 178-token Kokoro vocab. We **embed** it
//! ([`KOKORO_VOCAB`]) so an engine needs no `tokenizer.json` on disk; the English
//! engine still prefers the model's sibling `tokenizer.json` when present (exact
//! parity with that export), falling back to the embedded copy.
//!
//! Each engine's ONNX session is built **lazily** on first synthesis, so loading
//! both languages costs nothing until a voice from that language is actually used.
//!
//! ## Synthesis pipeline (per engine)
//!
//! 1. **Phonemize** the text by invoking the **espeak-ng binary**
//!    (`espeak-ng -q --ipa --tie=^ -v <voice>`) — continuous IPA with multi-letter
//!    phonemes tied by `^`, exactly what misaki's phonemizer (`tie='^'`) emits and
//!    what the remap tables below expect. We call the executable rather than linking
//!    `libespeak-ng`, so there is no native build/link step on Windows.
//! 2. **Remap** espeak IPA to the Kokoro phoneme alphabet — the American-English
//!    table for the English dialect, the generic `EspeakG2P` table for German.
//! 3. **Tokenize** the phonemes greedily against the vocab (the vocab keys are
//!    single characters, so this is effectively per-character), capping the inner
//!    sequence at 510 tokens.
//! 4. **Infer**: the per-voice `style [1,256]` row is selected by the **inner**
//!    token count (without the BOS/EOS pads — matching the reference); feed
//!    `input_ids [1,N] i64`, `style`, and `speed [1] f32`; read the `waveform`.
//!
//! Execution: **CPU only**. Kokoro-82M is tiny and runs faster than real time on
//! CPU. We deliberately do *not* use the DirectML EP: the istftnet vocoder's
//! `ConvTranspose` op fails at runtime under DirectML (`E_INVALIDARG`), and ORT
//! can't fall back per-node once a node is assigned to DML (verified 2026-06-23 on
//! a DX12 GPU). CUDA would work but its redistributable is too heavy to bundle.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use ort::execution_providers::CPUExecutionProvider;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;

use crate::provider::{
    below_normal_priority, CancelToken, Capability, Health, LicenseInfo, ModelRequirement,
    Provider, ProviderError, ProviderResult,
};
use crate::tts::{TextToSpeechProvider, TtsRequest, TtsResponse, TtsVoice};

/// Kokoro output is fixed at 24 kHz mono (its native sample rate).
const SAMPLE_RATE: u32 = 24_000;

/// Each Kokoro voice style vector is 256 floats; a voice table is a flat
/// `[N, 256]` buffer indexed by token count.
const STYLE_DIM: usize = 256;

/// Kokoro caps the sequence at 512 tokens (510 inner + BOS/EOS pads).
const MODEL_MAX_TOKENS: usize = 512;

/// The BOS/EOS pad token id. It is `0` in every Kokoro vocab (the `$` token in the
/// onnx-community `tokenizer.json`; unnamed but still `0` in kokoro-onnx's config).
const PAD_ID: i64 = 0;

/// English (onnx-community v1.0) built-in voices we surface in the picker. A voice
/// is selectable only if its style data is present in the voices source.
const EN_VOICES: &[(&str, &str)] = &[
    ("af_heart", "Heart (AF)"),
    ("af_bella", "Bella (AF)"),
    ("am_michael", "Michael (AM)"),
    ("bf_emma", "Emma (BF)"),
    ("bm_george", "George (BM)"),
];

/// German (Godelaune Martin) display names. The model is single-speaker; the voice
/// id is the key inside `voices-martin.npz` (`martin`). Unknown ids are title-cased.
const DE_VOICES: &[(&str, &str)] = &[("martin", "Martin (DE)")];

/// The phonemization dialect for an engine — selects the espeak→Kokoro remap.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Dialect {
    /// American English: espeak `en-us` IPA → misaki English remap.
    English,
    /// Generic non-English (German): espeak `<lang>` IPA → misaki `EspeakG2P`
    /// remap (stress kept, no English-specific substitutions).
    Generic,
}

/// Which language an engine speaks — picks the model's dialect, espeak voice,
/// built-in voice labels and BCP-47 tag.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum KokoroLanguage {
    English,
    German,
}

impl KokoroLanguage {
    fn dialect(self) -> Dialect {
        match self {
            KokoroLanguage::English => Dialect::English,
            KokoroLanguage::German => Dialect::Generic,
        }
    }
    /// espeak-ng voice code for grapheme-to-phoneme.
    fn espeak_voice(self) -> &'static str {
        match self {
            KokoroLanguage::English => "en-us",
            KokoroLanguage::German => "de",
        }
    }
    /// Short language tag surfaced on each voice.
    fn tag(self) -> &'static str {
        match self {
            KokoroLanguage::English => "en",
            KokoroLanguage::German => "de",
        }
    }
    fn labels(self) -> &'static [(&'static str, &'static str)] {
        match self {
            KokoroLanguage::English => EN_VOICES,
            KokoroLanguage::German => DE_VOICES,
        }
    }
}

/// One Kokoro model + its voices and dialect. The ONNX session is built lazily on
/// first synthesis (see [`KokoroEngine::with_session`]).
struct KokoroEngine {
    /// Path to the `.onnx` model, used to build the session lazily.
    model_path: PathBuf,
    /// `ort::Session` is `Send` but not `Sync`, and inference takes `&mut`. Built
    /// on first use; `None` until then.
    session: Mutex<Option<Session>>,
    /// phoneme/char string → token id (the canonical Kokoro vocab).
    vocab: HashMap<String, i64>,
    /// Longest key (in chars) in `vocab`, for greedy longest-match tokenizing.
    max_token_chars: usize,
    /// Per-voice flat `[N, 256]` style table, keyed by voice id.
    voices: HashMap<String, Vec<f32>>,
    /// Voice metadata (id, display, language) for the picker — only loaded voices.
    voice_meta: Vec<TtsVoice>,
    /// The espeak-ng executable used for grapheme-to-phoneme.
    espeak: PathBuf,
    /// Optional `--path` argument: the directory containing `espeak-ng-data`, for a
    /// bundled/portable espeak-ng whose data isn't on the default search path.
    espeak_data: Option<PathBuf>,
    /// espeak voice code (`en-us`, `de`).
    espeak_voice: &'static str,
    /// Which remap to apply to espeak's IPA.
    dialect: Dialect,
    /// Default voice id (first built-in voice that's actually present).
    default_id: String,
}

impl KokoroEngine {
    fn build(
        model_path: PathBuf,
        voices_path: &Path,
        language: KokoroLanguage,
        espeak: PathBuf,
        espeak_data: Option<PathBuf>,
    ) -> Result<Self, String> {
        if !model_path.exists() {
            return Err(format!("model not found: {}", model_path.display()));
        }
        // English prefers the model's own tokenizer.json (exact parity); German
        // ships none, so both fall back to the embedded canonical vocab.
        let vocab = match language {
            KokoroLanguage::English => load_vocab(&model_path).unwrap_or_else(|_| embedded_vocab()),
            KokoroLanguage::German => embedded_vocab(),
        };
        let max_token_chars = vocab.keys().map(|k| k.chars().count()).max().unwrap_or(1);

        let voices = load_voices(voices_path)?;
        if voices.is_empty() {
            return Err(format!(
                "no Kokoro voices found in {}",
                voices_path.display()
            ));
        }

        let labels = language.labels();
        let default_id = labels
            .iter()
            .map(|(id, _)| *id)
            .find(|id| voices.contains_key(*id))
            .map(str::to_string)
            .or_else(|| {
                let mut keys: Vec<&String> = voices.keys().collect();
                keys.sort();
                keys.first().map(|s| (*s).clone())
            })
            .unwrap_or_default();

        let voice_meta = voice_metadata(&voices, language);

        Ok(Self {
            model_path,
            session: Mutex::new(None),
            vocab,
            max_token_chars,
            voices,
            voice_meta,
            espeak,
            espeak_data,
            espeak_voice: language.espeak_voice(),
            dialect: language.dialect(),
            default_id,
        })
    }

    fn has_voice(&self, voice_id: &str) -> bool {
        self.voices.contains_key(voice_id)
    }

    /// Text → Kokoro phonemes via the espeak-ng binary, then the dialect remap.
    fn phonemize(&self, text: &str) -> ProviderResult<String> {
        // The generic (German) dialect mirrors misaki's EspeakG2P preprocessing:
        // swap real parens out of the way so they survive (any `()` left in the
        // espeak output are language-switch flags, which the remap strips).
        let prepared = match self.dialect {
            Dialect::English => text.to_string(),
            Dialect::Generic => text
                .replace('«', "\u{201C}")
                .replace('»', "\u{201D}")
                .replace('(', "«")
                .replace(')', "»"),
        };

        let mut command = Command::new(&self.espeak);
        if let Some(data) = &self.espeak_data {
            // Point a bundled/portable espeak-ng at its data dir (the directory
            // that contains `espeak-ng-data`).
            command.arg("--path").arg(data);
        }
        command
            .arg("-q") // quiet: don't synthesize audio
            .arg("--ipa") // IPA phonemes, continuous (no `_` separators)
            .arg("--tie=^") // tie multi-letter phonemes with `^` (misaki's convention)
            .arg("-v")
            .arg(self.espeak_voice)
            .arg(&prepared);
        below_normal_priority(&mut command);
        let output = command
            .output()
            .map_err(|e| ProviderError::Runtime(format!("run espeak-ng: {e}")))?;
        if !output.status.success() {
            return Err(ProviderError::Runtime(format!(
                "espeak-ng exited with {}",
                output.status
            )));
        }
        let ipa = String::from_utf8_lossy(&output.stdout);
        // espeak prints one line per sentence; join with spaces and collapse
        // surrounding whitespace before the remap.
        let ipa = ipa.split_whitespace().collect::<Vec<_>>().join(" ");
        let mapped = match self.dialect {
            Dialect::English => espeak_ipa_to_misaki(ipa.trim()),
            Dialect::Generic => espeak_ipa_to_misaki_generic(ipa.trim()),
        };
        Ok(mapped)
    }

    /// Greedy longest-match tokenize of the phonemes against the vocab. The vocab
    /// keys are single characters, so this is effectively per-character. Returns
    /// the **inner** tokens (no pads), capped at 510.
    fn tokenize_inner(&self, phonemes: &str) -> Vec<i64> {
        let chars: Vec<char> = phonemes.chars().collect();
        let mut out: Vec<i64> = Vec::with_capacity(chars.len());
        let cap = MODEL_MAX_TOKENS - 2;
        let mut i = 0;
        while i < chars.len() && out.len() < cap {
            let limit = self.max_token_chars.min(chars.len() - i);
            let mut matched = false;
            for len in (1..=limit).rev() {
                let cand: String = chars[i..i + len].iter().collect();
                if let Some(&id) = self.vocab.get(&cand) {
                    out.push(id);
                    i += len;
                    matched = true;
                    break;
                }
            }
            if !matched {
                // Skip unknown chars (whitespace and any phoneme not in vocab).
                i += 1;
            }
        }
        out
    }

    /// The per-voice style row for a sequence of `inner_len` tokens (excluding the
    /// BOS/EOS pads, matching the reference `voice[len(tokens)]`). Out-of-range
    /// counts re-use the last row.
    fn style_for(&self, voice_id: &str, inner_len: usize) -> ProviderResult<Vec<f32>> {
        let table = self
            .voices
            .get(voice_id)
            .ok_or_else(|| ProviderError::Runtime(format!("voice not loaded: {voice_id}")))?;
        let rows = table.len() / STYLE_DIM;
        if rows == 0 {
            return Err(ProviderError::Runtime(format!(
                "voice {voice_id} has no style rows"
            )));
        }
        let idx = inner_len.min(rows - 1);
        let offset = idx * STYLE_DIM;
        Ok(table[offset..offset + STYLE_DIM].to_vec())
    }

    /// Build the ONNX session if it hasn't been built yet, then run `f` with it.
    fn with_session<R>(
        &self,
        f: impl FnOnce(&mut Session) -> ProviderResult<R>,
    ) -> ProviderResult<R> {
        let mut guard = self
            .session
            .lock()
            .map_err(|e| ProviderError::Runtime(format!("kokoro session lock: {e}")))?;
        if guard.is_none() {
            let session = build_session(&self.model_path).map_err(ProviderError::Runtime)?;
            *guard = Some(session);
        }
        let session = guard
            .as_mut()
            .ok_or_else(|| ProviderError::Runtime("kokoro session missing".into()))?;
        f(session)
    }

    fn synthesize(
        &self,
        text: &str,
        voice_id: &str,
        speed: f32,
        cancel: &CancelToken,
    ) -> ProviderResult<TtsResponse> {
        let phonemes = self.phonemize(text)?;
        let inner = self.tokenize_inner(&phonemes);
        if inner.is_empty() {
            return Ok(TtsResponse {
                samples: Vec::new(),
                sample_rate: SAMPLE_RATE,
            });
        }
        if cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }

        // Style row is indexed by the INNER token count (before adding pads).
        let style = self.style_for(voice_id, inner.len())?;

        let mut tokens = Vec::with_capacity(inner.len() + 2);
        tokens.push(PAD_ID);
        tokens.extend_from_slice(&inner);
        tokens.push(PAD_ID);

        let token_count = tokens.len();
        let input_ids = Tensor::from_array(([1_i64, token_count as i64], tokens))
            .map_err(|e| ProviderError::Runtime(format!("kokoro input_ids tensor: {e}")))?;
        let style_tensor = Tensor::from_array(([1_i64, STYLE_DIM as i64], style))
            .map_err(|e| ProviderError::Runtime(format!("kokoro style tensor: {e}")))?;
        let speed_tensor = Tensor::from_array(([1_i64], vec![speed]))
            .map_err(|e| ProviderError::Runtime(format!("kokoro speed tensor: {e}")))?;

        self.with_session(|session| {
            // The token input is named `input_ids` on newer exports (onnx-community
            // v1.0) and `tokens` on older ones (the German Martin model). Pick the
            // one the loaded model actually declares.
            let token_name = if session.inputs.iter().any(|i| i.name == "input_ids") {
                "input_ids"
            } else {
                "tokens"
            };
            let outputs = session
                .run(ort::inputs![
                    token_name => input_ids,
                    "style" => style_tensor,
                    "speed" => speed_tensor,
                ])
                .map_err(|e| ProviderError::Runtime(format!("kokoro inference: {e}")))?;

            // The audio output is named "waveform" (v1.0). Accept "audio" as a
            // fallback name for robustness against re-exported variants.
            let audio = outputs
                .get("waveform")
                .or_else(|| outputs.get("audio"))
                .ok_or_else(|| ProviderError::Runtime("kokoro output missing waveform".into()))?;
            let samples = audio
                .try_extract_tensor::<f32>()
                .map_err(|e| ProviderError::Runtime(format!("kokoro output extract: {e}")))?
                .1
                .to_vec();
            Ok(TtsResponse {
                samples,
                sample_rate: SAMPLE_RATE,
            })
        })
    }
}

/// The native Kokoro provider: one or more language engines behind a single
/// provider (so the voice picker shows all languages' voices under "kokoro").
pub struct KokoroTts {
    engines: Vec<KokoroEngine>,
}

/// A single engine to load: model, voices source, and the language it speaks.
pub struct KokoroEngineConfig {
    pub model_path: PathBuf,
    pub voices_path: PathBuf,
    pub language: KokoroLanguage,
}

impl KokoroTts {
    /// Build a single English engine from a model + voices source. Kept for
    /// back-compat (tests, simple callers); prefer [`KokoroTts::load`] for
    /// multi-language setups.
    pub fn new(
        model_path: impl AsRef<Path>,
        voices_path: impl AsRef<Path>,
    ) -> Result<Self, String> {
        Self::load(vec![KokoroEngineConfig {
            model_path: model_path.as_ref().to_path_buf(),
            voices_path: voices_path.as_ref().to_path_buf(),
            language: KokoroLanguage::English,
        }])
    }

    /// Build the provider from one or more engine configs. Engines whose assets
    /// are missing are skipped (logged); fails only when no engine could be built
    /// — so the caller can fall back to another TTS provider.
    pub fn load(configs: Vec<KokoroEngineConfig>) -> Result<Self, String> {
        let espeak = resolve_espeak().ok_or_else(|| {
            "espeak-ng not found (set EXOQUILL_ESPEAK or install espeak-ng)".to_string()
        })?;
        // Optional data dir for a bundled/portable espeak-ng (passed as `--path`).
        let espeak_data = std::env::var_os("EXOQUILL_ESPEAK_DATA")
            .map(PathBuf::from)
            .filter(|p| p.exists());

        let mut engines = Vec::new();
        let mut last_err = None;
        for config in configs {
            match KokoroEngine::build(
                config.model_path,
                &config.voices_path,
                config.language,
                espeak.clone(),
                espeak_data.clone(),
            ) {
                Ok(engine) => engines.push(engine),
                Err(error) => {
                    eprintln!("kokoro engine unavailable: {error}");
                    last_err = Some(error);
                }
            }
        }

        if engines.is_empty() {
            return Err(last_err.unwrap_or_else(|| "no Kokoro engines configured".into()));
        }
        Ok(Self { engines })
    }

    /// Find the engine that owns `voice_id`, or `None` when no engine has it.
    fn engine_for(&self, voice_id: &str) -> Option<&KokoroEngine> {
        let v = voice_id.trim();
        if v.is_empty() {
            return None;
        }
        self.engines.iter().find(|e| e.has_voice(v))
    }
}

impl Provider for KokoroTts {
    fn id(&self) -> &str {
        "tts.kokoro"
    }
    fn display_name(&self) -> &str {
        "Kokoro-82M (nativ)"
    }
    fn version(&self) -> &str {
        "1"
    }
    fn capabilities(&self) -> Vec<Capability> {
        Vec::new()
    }
    fn required_models(&self) -> Vec<ModelRequirement> {
        vec![ModelRequirement {
            model_id: "tts.kokoro_82m".into(),
            feature: "tts".into(),
            required: true,
        }]
    }
    fn license_info(&self) -> LicenseInfo {
        LicenseInfo {
            runtime_license: "Apache-2.0".into(),
            source: Some("hexgrad/Kokoro-82M".into()),
        }
    }
    fn health_check(&self) -> Health {
        // Constructed only with at least one engine whose model + voices + espeak
        // resolved, so a live instance is ready.
        Health::Ready
    }
}

impl TextToSpeechProvider for KokoroTts {
    fn run(&self, request: TtsRequest, cancel: &CancelToken) -> ProviderResult<TtsResponse> {
        if cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        let text = request.text.trim();
        if text.is_empty() {
            return Ok(TtsResponse {
                samples: Vec::new(),
                sample_rate: SAMPLE_RATE,
            });
        }

        // Route to the engine owning the requested voice; otherwise fall back to
        // the first engine and its default voice.
        let (engine, voice_id) = match self.engine_for(&request.voice_id) {
            Some(engine) => (engine, request.voice_id.trim().to_string()),
            None => {
                let engine = self
                    .engines
                    .first()
                    .ok_or_else(|| ProviderError::Runtime("no kokoro engine".into()))?;
                (engine, engine.default_id.clone())
            }
        };

        let speed = if request.speed > 0.0 {
            request.speed
        } else {
            1.0
        };
        engine.synthesize(text, &voice_id, speed, cancel)
    }

    fn voices(&self) -> Vec<TtsVoice> {
        self.engines
            .iter()
            .flat_map(|e| e.voice_meta.iter().cloned())
            .collect()
    }

    fn default_voice(&self) -> Option<String> {
        self.engines
            .first()
            .map(|e| e.default_id.clone())
            .filter(|s| !s.is_empty())
    }
}

/// Build voice metadata (for the picker) from the loaded voices and language.
fn voice_metadata(voices: &HashMap<String, Vec<f32>>, language: KokoroLanguage) -> Vec<TtsVoice> {
    let labels = language.labels();
    // Stable order: known labels first (in their table order), then any extras
    // sorted by id.
    let mut metas = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (id, display) in labels {
        if voices.contains_key(*id) {
            metas.push(make_voice(id, display, language));
            seen.insert((*id).to_string());
        }
    }
    let mut extras: Vec<&String> = voices.keys().filter(|k| !seen.contains(*k)).collect();
    extras.sort();
    for id in extras {
        let display = title_case(id);
        metas.push(make_voice(id, &display, language));
    }
    metas
}

fn make_voice(id: &str, display: &str, language: KokoroLanguage) -> TtsVoice {
    TtsVoice {
        id: id.to_string(),
        display_name: display.to_string(),
        language: language.tag().to_string(),
        quality: "kokoro".into(),
        provider: "kokoro".into(),
    }
}

/// Title-case a voice id like `martin` → `Martin (DE)`-ish fallback label.
fn title_case(id: &str) -> String {
    let mut chars = id.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Build a Kokoro ONNX session on the **CPU** execution provider. DirectML is
/// intentionally not used — the istftnet vocoder's `ConvTranspose` op fails at
/// runtime under the DML EP — and CPU is faster than real time for this 82M model.
fn build_session(model_path: &Path) -> Result<Session, String> {
    Session::builder()
        .map_err(|e| format!("ort session builder: {e}"))?
        .with_execution_providers([CPUExecutionProvider::default().build()])
        .map_err(|e| format!("ort execution providers: {e}"))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| format!("ort optimization level: {e}"))?
        .commit_from_file(model_path)
        .map_err(|e| format!("load kokoro model: {e}"))
}

/// Resolve the espeak-ng executable: `EXOQUILL_ESPEAK`, then PATH, then the usual
/// Windows install locations. Returns `None` if none is usable.
fn resolve_espeak() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("EXOQUILL_ESPEAK") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    // A bare command name lets the OS resolve it on PATH at spawn time; probe it
    // with `--version` so we only commit to it when it actually runs.
    for name in ["espeak-ng", "espeak"] {
        if Command::new(name)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Some(PathBuf::from(name));
        }
    }
    #[cfg(windows)]
    for candidate in [
        r"C:\Program Files\eSpeak NG\espeak-ng.exe",
        r"C:\Program Files (x86)\eSpeak NG\espeak-ng.exe",
    ] {
        let p = PathBuf::from(candidate);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Load the tokenizer vocab (`model.vocab`) from the `tokenizer.json` next to the
/// model (or one directory up — the HF repo keeps it at the root, the model under
/// `onnx/`).
fn load_vocab(model_path: &Path) -> Result<HashMap<String, i64>, String> {
    let dir = model_path.parent().unwrap_or_else(|| Path::new("."));
    let candidates = [
        dir.join("tokenizer.json"),
        dir.join("..").join("tokenizer.json"),
    ];
    let path = candidates
        .iter()
        .find(|p| p.exists())
        .ok_or_else(|| format!("tokenizer.json not found near {}", model_path.display()))?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("read tokenizer.json: {e}"))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("parse tokenizer.json: {e}"))?;
    let vocab_obj = json
        .get("model")
        .and_then(|m| m.get("vocab"))
        .and_then(|v| v.as_object())
        .ok_or_else(|| "tokenizer.json has no model.vocab object".to_string())?;
    let mut vocab = HashMap::with_capacity(vocab_obj.len());
    for (token, id) in vocab_obj {
        if let Some(id) = id.as_i64() {
            vocab.insert(token.clone(), id);
        }
    }
    if vocab.is_empty() {
        return Err("tokenizer vocab is empty".into());
    }
    Ok(vocab)
}

/// The canonical 178-token Kokoro vocab (hexgrad/Kokoro, shared by the
/// onnx-community and kokoro-onnx exports). Embedded so an engine needs no
/// `tokenizer.json` on disk. Pad `$` is `0` (see [`PAD_ID`]) and omitted here.
fn embedded_vocab() -> HashMap<String, i64> {
    KOKORO_VOCAB
        .iter()
        .map(|(tok, id)| ((*tok).to_string(), *id))
        .collect()
}

/// The canonical Kokoro vocab entries (single-character keys → token id).
const KOKORO_VOCAB: &[(&str, i64)] = &[
    (";", 1),
    (":", 2),
    (",", 3),
    (".", 4),
    ("!", 5),
    ("?", 6),
    ("—", 9),
    ("…", 10),
    ("\"", 11),
    ("(", 12),
    (")", 13),
    ("\u{201C}", 14), // “
    ("\u{201D}", 15), // ”
    (" ", 16),
    ("\u{0303}", 17), // combining tilde
    ("ʣ", 18),
    ("ʥ", 19),
    ("ʦ", 20),
    ("ʨ", 21),
    ("ᵝ", 22),
    ("\u{AB67}", 23), // ꭧ
    ("A", 24),
    ("I", 25),
    ("O", 31),
    ("Q", 33),
    ("S", 35),
    ("T", 36),
    ("W", 39),
    ("Y", 41),
    ("ᵊ", 42),
    ("a", 43),
    ("b", 44),
    ("c", 45),
    ("d", 46),
    ("e", 47),
    ("f", 48),
    ("h", 50),
    ("i", 51),
    ("j", 52),
    ("k", 53),
    ("l", 54),
    ("m", 55),
    ("n", 56),
    ("o", 57),
    ("p", 58),
    ("q", 59),
    ("r", 60),
    ("s", 61),
    ("t", 62),
    ("u", 63),
    ("v", 64),
    ("w", 65),
    ("x", 66),
    ("y", 67),
    ("z", 68),
    ("ɑ", 69),
    ("ɐ", 70),
    ("ɒ", 71),
    ("æ", 72),
    ("β", 75),
    ("ɔ", 76),
    ("ɕ", 77),
    ("ç", 78),
    ("ɖ", 80),
    ("ð", 81),
    ("ʤ", 82),
    ("ə", 83),
    ("ɚ", 85),
    ("ɛ", 86),
    ("ɜ", 87),
    ("ɟ", 90),
    ("ɡ", 92),
    ("ɥ", 99),
    ("ɨ", 101),
    ("ɪ", 102),
    ("ʝ", 103),
    ("ɯ", 110),
    ("ɰ", 111),
    ("ŋ", 112),
    ("ɳ", 113),
    ("ɲ", 114),
    ("ɴ", 115),
    ("ø", 116),
    ("ɸ", 118),
    ("θ", 119),
    ("œ", 120),
    ("ɹ", 123),
    ("ɾ", 125),
    ("ɻ", 126),
    ("ʁ", 128),
    ("ɽ", 129),
    ("ʂ", 130),
    ("ʃ", 131),
    ("ʈ", 132),
    ("ʧ", 133),
    ("ʊ", 135),
    ("ʋ", 136),
    ("ʌ", 138),
    ("ɣ", 139),
    ("ɤ", 140),
    ("χ", 142),
    ("ʎ", 143),
    ("ʒ", 147),
    ("ʔ", 148),
    ("ˈ", 156),
    ("ˌ", 157),
    ("ː", 158),
    ("ʰ", 162),
    ("ʲ", 164),
    ("↓", 169),
    ("→", 171),
    ("↗", 172),
    ("↘", 173),
    ("ᵻ", 177),
];

/// Load voice style tables. `path` may be:
/// - a directory of `<name>.bin` and/or `<name>.npz` files,
/// - a single `<name>.bin` (flat little-endian `[N, 256]` f32), or
/// - a single `.npz` (NumPy archive of `<name>.npy` `[N, .., 256]` f32 arrays).
fn load_voices(path: &Path) -> Result<HashMap<String, Vec<f32>>, String> {
    let mut voices = HashMap::new();
    if path.is_dir() {
        let entries = std::fs::read_dir(path).map_err(|e| format!("read voices dir: {e}"))?;
        for entry in entries.flatten() {
            let file = entry.path();
            match file.extension().and_then(|e| e.to_str()) {
                Some("bin") => {
                    if let (Some(stem), Some(data)) = (
                        file.file_stem().and_then(|s| s.to_str()),
                        read_voice_bin(&file),
                    ) {
                        voices.insert(stem.to_string(), data);
                    }
                }
                Some("npz") => {
                    if let Ok(bytes) = std::fs::read(&file) {
                        for (name, data) in parse_npz(&bytes) {
                            voices.insert(name, data);
                        }
                    }
                }
                _ => {}
            }
        }
    } else if path.extension().and_then(|e| e.to_str()) == Some("npz") {
        let bytes = std::fs::read(path).map_err(|e| format!("read voices npz: {e}"))?;
        for (name, data) in parse_npz(&bytes) {
            voices.insert(name, data);
        }
    } else if path.extension().and_then(|e| e.to_str()) == Some("bin") {
        if let (Some(stem), Some(data)) = (
            path.file_stem().and_then(|s| s.to_str()),
            read_voice_bin(path),
        ) {
            voices.insert(stem.to_string(), data);
        }
    }
    Ok(voices)
}

/// Read a voice `.bin` as little-endian f32. Returns `None` when the file can't be
/// read or its length isn't a whole number of 256-float style rows.
fn read_voice_bin(path: &Path) -> Option<Vec<f32>> {
    let bytes = std::fs::read(path).ok()?;
    bytes_to_f32_rows(&bytes)
}

/// Interpret a little-endian f32 byte buffer as a flat `[N, 256]` style table.
/// `None` unless the length is a whole number of 256-float rows.
fn bytes_to_f32_rows(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.is_empty() || !bytes.len().is_multiple_of(STYLE_DIM * 4) {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect(),
    )
}

/// Parse a NumPy `.npz` (a ZIP archive of `.npy` arrays) into voice tables keyed
/// by the entry name (without `.npy`). Only **stored** (uncompressed) entries are
/// supported — that's what `numpy.savez` writes by default, and how
/// `voices-martin.npz` ships. Entries that aren't little-endian f32 with a last
/// dim of 256 are skipped.
fn parse_npz(bytes: &[u8]) -> Vec<(String, Vec<f32>)> {
    const LOCAL_HEADER: [u8; 4] = [0x50, 0x4b, 0x03, 0x04]; // PK\x03\x04
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos + 30 <= bytes.len() && bytes[pos..pos + 4] == LOCAL_HEADER {
        let read_u16 = |off: usize| u16::from_le_bytes([bytes[off], bytes[off + 1]]) as usize;
        let read_u32 = |off: usize| {
            u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
                as usize
        };
        let method = read_u16(pos + 8);
        let comp_size = read_u32(pos + 18);
        let name_len = read_u16(pos + 26);
        let extra_len = read_u16(pos + 28);
        let name_start = pos + 30;
        let data_start = name_start + name_len + extra_len;
        if data_start + comp_size > bytes.len() {
            break;
        }
        let name = String::from_utf8_lossy(&bytes[name_start..name_start + name_len]).to_string();
        // Only stored (method 0); deflate (8) would need an inflater dependency.
        if method == 0 {
            let entry = &bytes[data_start..data_start + comp_size];
            if let Some(stem) = name.strip_suffix(".npy") {
                if let Some(rows) = parse_npy_f32(entry) {
                    out.push((stem.to_string(), rows));
                }
            }
        }
        pos = data_start + comp_size;
    }
    out
}

/// Parse a `.npy` v1/v2 buffer of little-endian f32 (`<f4`) into a flat row buffer
/// (the leading dims collapse; only a last dim of 256 is meaningful here). Returns
/// `None` on a non-f32 dtype or a length that isn't a whole number of 256 rows.
fn parse_npy_f32(bytes: &[u8]) -> Option<Vec<f32>> {
    const MAGIC: &[u8] = b"\x93NUMPY";
    if bytes.len() < 10 || &bytes[0..6] != MAGIC {
        return None;
    }
    let major = bytes[6];
    let (header_len, header_start) = if major >= 2 {
        if bytes.len() < 12 {
            return None;
        }
        (
            u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize,
            12,
        )
    } else {
        (u16::from_le_bytes([bytes[8], bytes[9]]) as usize, 10)
    };
    let data_start = header_start + header_len;
    if data_start > bytes.len() {
        return None;
    }
    let header = String::from_utf8_lossy(&bytes[header_start..data_start]);
    // Require little-endian float32. (Big-endian / f64 voices aren't a thing for
    // Kokoro, so reject rather than mis-read.)
    if !(header.contains("'<f4'") || header.contains("\"<f4\"")) {
        return None;
    }
    bytes_to_f32_rows(&bytes[data_start..])
}

/// Convert raw espeak-ng IPA into the Misaki phoneme alphabet for American
/// English, matching the reference `misaki` package's `FROM_ESPEAKS` table.
///
/// The fixed substitutions are applied longest-key-first (as in misaki), then
/// syllabic consonants and length marks are normalized.
fn espeak_ipa_to_misaki(ipa: &str) -> String {
    // The binary emits `^` ties directly (--tie=^); normalize any U+0361 tie bar to
    // `^` too, so the multi-letter diphthong keys below match either form.
    let mut result = ipa.replace('\u{0361}', "^");

    // FROM_ESPEAKS, longest key first (misaki sorts by -len(key)).
    const FROM_ESPEAKS: &[(&str, &str)] = &[
        ("ʔˌn\u{0329}", "tᵊn"),
        ("a^ɪ", "I"),
        ("a^ʊ", "W"),
        ("d^ʒ", "ʤ"),
        ("e^ɪ", "A"),
        ("t^ʃ", "ʧ"),
        ("ɔ^ɪ", "Y"),
        ("ə^l", "ᵊl"),
        ("ʔn", "tᵊn"),
        ("ɚ", "əɹ"),
        ("ʲO", "jO"),
        ("ʲQ", "jQ"),
        ("\u{0303}", ""),
        ("e", "A"),
        ("r", "ɹ"),
        ("x", "k"),
        ("ç", "k"),
        ("ɐ", "ə"),
        ("ɬ", "l"),
        ("ʔ", "t"),
        ("ʲ", ""),
    ];
    for (from, to) in FROM_ESPEAKS {
        if result.contains(from) {
            result = result.replace(from, to);
        }
    }

    // Syllabic consonant: `<C>̩` → `ᵊ<C>`.
    let mut chars: Vec<char> = result.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if i + 1 < chars.len() && chars[i + 1] == '\u{0329}' {
            let consonant = chars[i];
            chars[i] = 'ᵊ';
            chars[i + 1] = consonant;
            i += 2;
        } else {
            i += 1;
        }
    }
    result = chars.into_iter().collect();
    result = result.replace('\u{0329}', "");

    // American English specifics + length-mark removal.
    result = result.replace("o^ʊ", "O");
    result = result.replace("ɜːɹ", "ɜɹ");
    result = result.replace("ɜː", "ɜɹ");
    result = result.replace("ɪə", "iə");
    result = result.replace('ː', "");
    result = result.replace('^', "");

    result
}

/// Convert raw espeak-ng IPA into the Kokoro phoneme alphabet for non-English
/// languages (German), matching misaki's generic `EspeakG2P` remap: keep stress
/// marks, apply the small diphthong/affricate table, strip language-switch flags
/// and tie/hyphen artifacts. No American-English FROM_ESPEAKS substitutions.
fn espeak_ipa_to_misaki_generic(ipa: &str) -> String {
    // The binary emits `^` ties directly (--tie=^); normalize any U+0361 tie bar to
    // `^` too (the misaki `tie='^'` convention).
    let mut s = ipa.replace('\u{0361}', "^");

    // Drop espeak language-switch flags like "(en)". Real parens were swapped to
    // «» before phonemizing, so any `(...)` left here are flags.
    s = strip_paren_groups(&s);

    // EspeakG2P.e2m (default version), applied as misaki does.
    const E2M: &[(&str, &str)] = &[
        ("a^ɪ", "I"),
        ("a^ʊ", "W"),
        ("d^z", "ʣ"),
        ("d^ʒ", "ʤ"),
        ("e^ɪ", "A"),
        ("o^ʊ", "O"),
        ("ə^ʊ", "Q"),
        ("s^s", "S"),
        ("t^s", "ʦ"),
        ("t^ʃ", "ʧ"),
        ("ɔ^ɪ", "Y"),
    ];
    for (from, to) in E2M {
        if s.contains(from) {
            s = s.replace(from, to);
        }
    }

    // Delete remaining tie chars and hyphens (misaki's non-2.0 path).
    s = s.replace('^', "");
    s = s.replace('-', "");

    // Restore the swapped-out real parentheses.
    s = s.replace('«', "(").replace('»', ")");
    s
}

/// Remove every `(...)` group from `s` (espeak language-switch flags). Unmatched
/// `(` drops the rest; this is only applied after real parens were swapped to «».
fn strip_paren_groups(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0u32;
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn misaki_maps_diphthong_and_consumes_ties() {
        // espeak tie bar U+0361 between o and ʊ → Misaki "O" (American English).
        let ipa = "o\u{0361}\u{028a}"; // o͡ʊ
        let out = espeak_ipa_to_misaki(ipa);
        assert!(!out.contains('^'), "tie marker not consumed: {out:?}");
        assert!(out.contains('O'), "expected o^ʊ→O mapping, got {out:?}");
    }

    #[test]
    fn misaki_removes_length_marks() {
        let out = espeak_ipa_to_misaki("w\u{025c}\u{02d0}\u{0279}ld"); // wɜːɹld-ish
        assert!(!out.contains('ː'), "length mark not removed: {out:?}");
    }

    #[test]
    fn generic_remap_keeps_stress_and_maps_affricates() {
        // German espeak output keeps stress (ˈ) and uses tie bars; the generic
        // remap maps t͡s → ʦ and o͡ʊ → O, removes ties, keeps ˈ.
        let ipa = "ˈt\u{0361}so\u{0361}\u{028a}"; // ˈt͡so͡ʊ
        let out = espeak_ipa_to_misaki_generic(ipa);
        assert!(out.contains('ˈ'), "stress mark dropped: {out:?}");
        assert!(out.contains('ʦ'), "expected t^s→ʦ, got {out:?}");
        assert!(out.contains('O'), "expected o^ʊ→O, got {out:?}");
        assert!(!out.contains('^'), "tie marker not consumed: {out:?}");
    }

    #[test]
    fn generic_remap_strips_language_flags() {
        // A swapped-out real paren round-trips; a bare flag group is removed.
        let out = espeak_ipa_to_misaki_generic("halo(en)world");
        assert_eq!(out, "haloworld", "language flag not stripped: {out:?}");
    }

    #[test]
    fn embedded_vocab_has_core_symbols() {
        let vocab = embedded_vocab();
        // A few load-bearing entries; pad ($/0) is implicit, not in the table.
        assert_eq!(vocab.get(" ").copied(), Some(16));
        assert_eq!(vocab.get("ˈ").copied(), Some(156));
        assert_eq!(vocab.get("ʦ").copied(), Some(20));
        assert!(
            vocab.values().all(|&id| id != 0),
            "pad id 0 must be reserved"
        );
    }

    #[test]
    fn read_voice_bin_rejects_misaligned() {
        // Not a whole number of 256-float rows → None.
        let path = std::env::temp_dir().join("exoquill_kokoro_bad_voice.bin");
        std::fs::write(&path, [0u8; 7]).unwrap();
        assert!(read_voice_bin(&path).is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn parse_npy_reads_f32_rows() {
        // Two style rows of 256 floats, C-order, little-endian f32.
        let rows = 2usize;
        let mut data = Vec::new();
        for r in 0..rows {
            for c in 0..STYLE_DIM {
                data.extend_from_slice(&((r * STYLE_DIM + c) as f32).to_le_bytes());
            }
        }
        let header = format!(
            "{{'descr': '<f4', 'fortran_order': False, 'shape': ({rows}, 1, {STYLE_DIM}), }}"
        );
        let mut npy = Vec::new();
        npy.extend_from_slice(b"\x93NUMPY");
        npy.push(1); // major
        npy.push(0); // minor
        npy.extend_from_slice(&(header.len() as u16).to_le_bytes());
        npy.extend_from_slice(header.as_bytes());
        npy.extend_from_slice(&data);

        let parsed = parse_npy_f32(&npy).expect("npy should parse");
        assert_eq!(parsed.len(), rows * STYLE_DIM);
        assert_eq!(parsed[STYLE_DIM + 5], (STYLE_DIM + 5) as f32);
    }

    #[test]
    fn parse_npz_extracts_stored_entry() {
        // Build a minimal npy, wrap it in a stored ZIP local entry named
        // "martin.npy", and terminate with a central-directory signature so the
        // parser stops cleanly.
        let mut data = Vec::new();
        for c in 0..STYLE_DIM {
            data.extend_from_slice(&(c as f32).to_le_bytes());
        }
        let header =
            format!("{{'descr': '<f4', 'fortran_order': False, 'shape': (1, {STYLE_DIM}), }}");
        let mut npy = Vec::new();
        npy.extend_from_slice(b"\x93NUMPY");
        npy.push(1);
        npy.push(0);
        npy.extend_from_slice(&(header.len() as u16).to_le_bytes());
        npy.extend_from_slice(header.as_bytes());
        npy.extend_from_slice(&data);

        let name = b"martin.npy";
        let mut zip = Vec::new();
        zip.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]); // local file header
        zip.extend_from_slice(&20u16.to_le_bytes()); // version needed
        zip.extend_from_slice(&0u16.to_le_bytes()); // flags
        zip.extend_from_slice(&0u16.to_le_bytes()); // method: stored
        zip.extend_from_slice(&0u16.to_le_bytes()); // mod time
        zip.extend_from_slice(&0u16.to_le_bytes()); // mod date
        zip.extend_from_slice(&0u32.to_le_bytes()); // crc32
        zip.extend_from_slice(&(npy.len() as u32).to_le_bytes()); // compressed size
        zip.extend_from_slice(&(npy.len() as u32).to_le_bytes()); // uncompressed size
        zip.extend_from_slice(&(name.len() as u16).to_le_bytes()); // name len
        zip.extend_from_slice(&0u16.to_le_bytes()); // extra len
        zip.extend_from_slice(name);
        zip.extend_from_slice(&npy);
        zip.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]); // central dir → stop

        let voices = parse_npz(&zip);
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].0, "martin");
        assert_eq!(voices[0].1.len(), STYLE_DIM);
    }

    /// Real synthesis is exercised only when the model + voices are present and
    /// espeak-ng is installed (set EXOQUILL_KOKORO_MODEL / EXOQUILL_KOKORO_VOICES
    /// and ORT_DYLIB_PATH). Skipped otherwise — like the Silero inference test.
    #[test]
    fn synthesizes_when_assets_present() {
        let (Ok(model), Ok(voices)) = (
            std::env::var("EXOQUILL_KOKORO_MODEL"),
            std::env::var("EXOQUILL_KOKORO_VOICES"),
        ) else {
            eprintln!("skipping: EXOQUILL_KOKORO_MODEL / EXOQUILL_KOKORO_VOICES not set");
            return;
        };
        if !Path::new(&model).exists() {
            eprintln!("skipping: model not present");
            return;
        }
        let tts = match KokoroTts::new(&model, &voices) {
            Ok(tts) => tts,
            Err(e) => {
                eprintln!("skipping: KokoroTts::new failed ({e})");
                return;
            }
        };
        let voice = tts.default_voice().unwrap_or_default();
        let resp = tts
            .run(
                TtsRequest::new("Hello, world.", voice, 1.0),
                &CancelToken::new(),
            )
            .expect("synthesis failed");
        assert_eq!(resp.sample_rate, SAMPLE_RATE);
        assert!(!resp.samples.is_empty(), "no audio produced");
    }

    /// German (Martin) synthesis through the generic dialect + npz voices, end to
    /// end. Exercised only when EXOQUILL_KOKORO_DE_MODEL / EXOQUILL_KOKORO_DE_VOICES
    /// (plus ORT_DYLIB_PATH and a German-capable espeak-ng) are set; skipped
    /// otherwise. The likely tuning point if the audio sounds wrong is the espeak
    /// `de` → Kokoro phoneme remap (`espeak_ipa_to_misaki_generic`).
    #[test]
    fn synthesizes_german_when_assets_present() {
        let (Ok(model), Ok(voices)) = (
            std::env::var("EXOQUILL_KOKORO_DE_MODEL"),
            std::env::var("EXOQUILL_KOKORO_DE_VOICES"),
        ) else {
            eprintln!("skipping: EXOQUILL_KOKORO_DE_MODEL / EXOQUILL_KOKORO_DE_VOICES not set");
            return;
        };
        if !Path::new(&model).exists() {
            eprintln!("skipping: german model not present");
            return;
        }
        let tts = match KokoroTts::load(vec![KokoroEngineConfig {
            model_path: PathBuf::from(&model),
            voices_path: PathBuf::from(&voices),
            language: KokoroLanguage::German,
        }]) {
            Ok(tts) => tts,
            Err(e) => {
                eprintln!("skipping: KokoroTts::load failed ({e})");
                return;
            }
        };
        let voice = tts.default_voice().unwrap_or_default();
        eprintln!("german default voice: {voice:?}");
        let resp = tts
            .run(
                TtsRequest::new("Guten Tag, wie geht es Ihnen?", voice, 1.0),
                &CancelToken::new(),
            )
            .expect("synthesis failed");
        assert_eq!(resp.sample_rate, SAMPLE_RATE);
        assert!(!resp.samples.is_empty(), "no audio produced");
        // The output must be real, finite speech — not silence or NaNs.
        assert!(
            resp.samples.iter().all(|s| s.is_finite()),
            "non-finite samples"
        );
        let peak = resp.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let secs = resp.samples.len() as f32 / SAMPLE_RATE as f32;
        eprintln!(
            "german audio: {} samples, {secs:.2}s, peak {peak:.3}",
            resp.samples.len()
        );
        assert!(peak > 0.01, "audio is effectively silent (peak {peak})");
        // Optional: dump a 24 kHz mono 16-bit WAV to audition the voice by ear.
        if let Ok(out) = std::env::var("EXOQUILL_KOKORO_WAV_OUT") {
            std::fs::write(&out, encode_wav_16k(&resp.samples, SAMPLE_RATE)).unwrap();
            eprintln!("wrote {out}");
        }
    }

    /// Encode mono f32 PCM (`[-1,1]`) as a canonical 16-bit WAV — for the opt-in
    /// audition in `synthesizes_german_when_assets_present`.
    #[cfg(test)]
    fn encode_wav_16k(samples: &[f32], rate: u32) -> Vec<u8> {
        let data_len = (samples.len() * 2) as u32;
        let mut wav = Vec::with_capacity(44 + data_len as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate * 2).to_le_bytes()); // byte rate
        wav.extend_from_slice(&2u16.to_le_bytes()); // block align
        wav.extend_from_slice(&16u16.to_le_bytes()); // bits/sample
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        for &s in samples {
            let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            wav.extend_from_slice(&v.to_le_bytes());
        }
        wav
    }
}
