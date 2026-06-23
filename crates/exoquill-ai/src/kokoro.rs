//! Native Kokoro-82M text-to-speech provider (hexgrad/Kokoro), via ONNX Runtime.
//!
//! Kokoro is a lightweight (82 M parameter), Apache-2.0 English TTS model that
//! outputs 24 kHz mono PCM. This provider runs the
//! `onnx-community/Kokoro-82M-v1.0-ONNX` model directly through the `ort` crate
//! (the same `load-dynamic` ONNX Runtime the Silero VAD uses) — there is **no
//! Python sidecar**.
//!
//! The synthesis pipeline mirrors the reference Kokoro / Misaki implementation:
//!
//! 1. **Phonemize** the input text to IPA by invoking the **espeak-ng binary**
//!    (`espeak-ng -q --ipa=1 -v en-us`). We call the executable rather than
//!    linking `libespeak-ng`, so there is no native build/link step on Windows.
//! 2. **espeak → Misaki** character remapping (`espeak_ipa_to_misaki`), the same
//!    fixed substitution table the Python `misaki` package applies for American
//!    English.
//! 3. **Tokenize** the Misaki IPA by greedy longest-match against the vocab in
//!    the model's `tokenizer.json` (`model.vocab`), wrapped in BOS/EOS `$` (id 0).
//! 4. **Infer**: feed `input_ids [1,N] i64`, `style [1,256] f32` (the per-voice
//!    style row selected by token count), and `speed [1] f32` to the model; read
//!    the `waveform` f32 output.
//!
//! Voices are a FIXED built-in set (no reference `.wav` clips). Each voice is a
//! `<name>.bin` file — a flat little-endian `[N, 256]` f32 style table indexed by
//! token count. [`KokoroTts::new`] fails cleanly (a `Result`) when the model,
//! voices, ONNX Runtime, or espeak-ng binary is missing, so setup can fall back
//! to the other TTS providers.
//!
//! GPU: the session is built with the execution-provider chain `[DirectML, CPU]`,
//! so it uses the GPU via DirectML when the loaded `onnxruntime.dll` supports it
//! and transparently falls back to CPU otherwise (`ort`'s default best-effort
//! registration). Kokoro is small enough to be fast on CPU regardless.
//!
//! English only: the pure-binary espeak path is invoked with `en-us`. (German
//! support would need an espeak voice switch + a German-capable Kokoro voice,
//! which the official v1.0 model does not ship.)

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use ort::execution_providers::{CPUExecutionProvider, DirectMLExecutionProvider};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;

use crate::provider::{
    below_normal_priority, CancelToken, Capability, Health, LicenseInfo, ModelRequirement,
    Provider, ProviderError, ProviderResult,
};
use crate::tts::{TextToSpeechProvider, TtsRequest, TtsResponse, TtsVoice};

/// Kokoro output is fixed at 24 kHz mono (its native sample rate).
const SAMPLE_RATE: u32 = 24_000;

/// Each Kokoro voice style vector is 256 floats; the `.bin` voice file is a flat
/// `[N, 256]` table indexed by token count.
const STYLE_DIM: usize = 256;

/// Kokoro caps the sequence at 512 tokens (matching the reference tokenizer).
const MODEL_MAX_TOKENS: usize = 512;

/// Kokoro-82M's built-in voice set we surface in the picker. The model ships many
/// more; this is the practical default set (matching the former sidecar). A voice
/// is selectable only if its `<id>.bin` is present in the voices directory.
const VOICES: &[(&str, &str)] = &[
    ("af_heart", "Heart (AF)"),
    ("af_bella", "Bella (AF)"),
    ("am_michael", "Michael (AM)"),
    ("bf_emma", "Emma (BF)"),
    ("bm_george", "George (BM)"),
];

/// The native Kokoro provider: an ONNX session plus the tokenizer vocab, the
/// per-voice style tables, and the resolved espeak-ng binary.
pub struct KokoroTts {
    /// `ort::Session` is `Send` but not `Sync`, and inference takes `&mut`; the
    /// provider is shared behind an `Arc`, so guard the session with a `Mutex`.
    session: Mutex<Session>,
    /// `tokenizer.json` `model.vocab`: phoneme/char string → token id.
    vocab: HashMap<String, i64>,
    /// The longest key (in chars) in `vocab`, for greedy longest-match tokenizing.
    max_token_chars: usize,
    /// BOS/EOS token id (the `$` pad token, id 0 in the Kokoro vocab).
    pad_id: i64,
    /// Per-voice flat `[N, 256]` style table, keyed by voice id (file stem).
    voices: HashMap<String, Vec<f32>>,
    /// The espeak-ng executable used for grapheme-to-phoneme.
    espeak: PathBuf,
    /// Default voice id (first built-in voice that's actually present).
    default_id: String,
}

impl KokoroTts {
    /// Build the provider from the Kokoro ONNX model and a voices source.
    ///
    /// `model_path` is the `.onnx` model; its sibling `tokenizer.json` (or a
    /// `tokenizer.json` one directory up) supplies the vocab. `voices_path` may be
    /// a directory of `<name>.bin` files (recommended) or a single `<name>.bin`.
    ///
    /// Fails (a `Result`) when ONNX Runtime can't be loaded, the model/tokenizer
    /// can't be read, no voices are found, or the espeak-ng binary is missing —
    /// so the caller can fall back to another TTS provider.
    pub fn new(
        model_path: impl AsRef<Path>,
        voices_path: impl AsRef<Path>,
    ) -> Result<Self, String> {
        let model_path = model_path.as_ref();
        let espeak = resolve_espeak().ok_or_else(|| {
            "espeak-ng not found (set EXOQUILL_ESPEAK or install espeak-ng)".to_string()
        })?;

        let vocab = load_vocab(model_path)?;
        let pad_id = *vocab
            .get("$")
            .ok_or_else(|| "tokenizer vocab has no '$' pad token".to_string())?;
        let max_token_chars = vocab.keys().map(|k| k.chars().count()).max().unwrap_or(1);

        let voices = load_voices(voices_path.as_ref())?;
        if voices.is_empty() {
            return Err("no Kokoro voices found".into());
        }
        let default_id = VOICES
            .iter()
            .map(|(id, _)| *id)
            .find(|id| voices.contains_key(*id))
            .map(str::to_string)
            .or_else(|| voices.keys().next().cloned())
            .unwrap_or_default();

        // Build the session with the [DirectML, CPU] EP chain: GPU when the loaded
        // onnxruntime.dll supports DirectML, else a graceful CPU fallback (ort
        // registers EPs best-effort and warns rather than erroring on an
        // unavailable one).
        let session = Session::builder()
            .map_err(|e| format!("ort session builder: {e}"))?
            .with_execution_providers([
                DirectMLExecutionProvider::default().build(),
                CPUExecutionProvider::default().build(),
            ])
            .map_err(|e| format!("ort execution providers: {e}"))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| format!("ort optimization level: {e}"))?
            .commit_from_file(model_path)
            .map_err(|e| format!("load kokoro model: {e}"))?;

        Ok(Self {
            session: Mutex::new(session),
            vocab,
            max_token_chars,
            pad_id,
            voices,
            espeak,
            default_id,
        })
    }

    /// The selectable voices — the built-in set. Available without inference so
    /// the picker populates (e.g. while listing all backends' voices).
    pub fn voices_static() -> Vec<TtsVoice> {
        VOICES
            .iter()
            .map(|(id, display)| TtsVoice {
                id: (*id).to_string(),
                display_name: (*display).to_string(),
                language: "en".into(),
                quality: "kokoro".into(),
                provider: "kokoro".into(),
            })
            .collect()
    }

    /// Resolve the voice id for a request, falling back to the default voice.
    fn pick_voice(&self, voice_id: &str) -> String {
        let v = voice_id.trim();
        if !v.is_empty() && self.voices.contains_key(v) {
            v.to_string()
        } else {
            self.default_id.clone()
        }
    }

    /// Text → Misaki IPA via the espeak-ng binary, then the espeak→Misaki remap.
    fn phonemize(&self, text: &str) -> ProviderResult<String> {
        let mut command = Command::new(&self.espeak);
        command
            .arg("-q") // quiet: don't synthesize audio
            .arg("--ipa=1") // IPA phonemes joined with U+0361 tie bars
            .arg("-v")
            .arg("en-us")
            .arg(text);
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
        // surrounding whitespace before the Misaki remap.
        let ipa = ipa.split_whitespace().collect::<Vec<_>>().join(" ");
        Ok(espeak_ipa_to_misaki(ipa.trim()))
    }

    /// Greedy longest-match tokenize of Misaki IPA against the vocab, wrapped in
    /// BOS/EOS pad tokens and truncated to the model's max sequence length.
    fn tokenize(&self, ipa: &str) -> Vec<i64> {
        let chars: Vec<char> = ipa.chars().collect();
        let mut inner: Vec<i64> = Vec::with_capacity(chars.len());
        let mut i = 0;
        while i < chars.len() {
            let limit = self.max_token_chars.min(chars.len() - i);
            let mut matched = false;
            for len in (1..=limit).rev() {
                let cand: String = chars[i..i + len].iter().collect();
                if let Some(&id) = self.vocab.get(&cand) {
                    inner.push(id);
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

        let mut tokens = Vec::with_capacity(inner.len() + 2);
        tokens.push(self.pad_id);
        tokens.append(&mut inner);
        tokens.push(self.pad_id);
        if tokens.len() > MODEL_MAX_TOKENS {
            let keep = MODEL_MAX_TOKENS.saturating_sub(2);
            let mut truncated = Vec::with_capacity(MODEL_MAX_TOKENS);
            truncated.push(self.pad_id);
            truncated.extend_from_slice(&tokens[1..1 + keep]);
            truncated.push(self.pad_id);
            tokens = truncated;
        }
        tokens
    }

    /// The per-voice style row for a token sequence of `token_len` tokens. The
    /// voice `.bin` is a flat `[N, 256]` table; Kokoro indexes it by the token
    /// count (`voices[len(tokens)]`). Out-of-range counts re-use the last row.
    fn style_for(&self, voice_id: &str, token_len: usize) -> ProviderResult<Vec<f32>> {
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
        let idx = token_len.min(rows - 1);
        let offset = idx * STYLE_DIM;
        Ok(table[offset..offset + STYLE_DIM].to_vec())
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
        // Constructed only when the model + voices + espeak all resolved, so a
        // live instance is ready.
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

        let voice_id = self.pick_voice(&request.voice_id);
        let ipa = self.phonemize(text)?;
        let tokens = self.tokenize(&ipa);
        // Only BOS/EOS → nothing to say.
        if tokens.len() <= 2 {
            return Ok(TtsResponse {
                samples: Vec::new(),
                sample_rate: SAMPLE_RATE,
            });
        }
        if cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }

        let style = self.style_for(&voice_id, tokens.len())?;
        let speed = if request.speed > 0.0 {
            request.speed
        } else {
            1.0
        };

        let token_count = tokens.len();
        let input_ids = Tensor::from_array(([1_i64, token_count as i64], tokens))
            .map_err(|e| ProviderError::Runtime(format!("kokoro input_ids tensor: {e}")))?;
        let style_tensor = Tensor::from_array(([1_i64, STYLE_DIM as i64], style))
            .map_err(|e| ProviderError::Runtime(format!("kokoro style tensor: {e}")))?;
        let speed_tensor = Tensor::from_array(([1_i64], vec![speed]))
            .map_err(|e| ProviderError::Runtime(format!("kokoro speed tensor: {e}")))?;

        let mut session = self
            .session
            .lock()
            .map_err(|e| ProviderError::Runtime(format!("kokoro session lock: {e}")))?;
        let outputs = session
            .run(ort::inputs![
                "input_ids" => input_ids,
                "style" => style_tensor,
                "speed" => speed_tensor,
            ])
            .map_err(|e| ProviderError::Runtime(format!("kokoro inference: {e}")))?;

        // The model's audio output is named "waveform" (v1.0). Accept "audio" as a
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
    }

    fn voices(&self) -> Vec<TtsVoice> {
        // Only the built-in voices that are actually loaded.
        VOICES
            .iter()
            .filter(|(id, _)| self.voices.contains_key(*id))
            .map(|(id, display)| TtsVoice {
                id: (*id).to_string(),
                display_name: (*display).to_string(),
                language: "en".into(),
                quality: "kokoro".into(),
                provider: "kokoro".into(),
            })
            .collect()
    }

    fn default_voice(&self) -> Option<String> {
        Some(self.default_id.clone())
    }
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

/// Load voice style tables. `path` may be a directory of `<name>.bin` files or a
/// single `<name>.bin`. Each file is a flat little-endian `[N, 256]` f32 buffer.
fn load_voices(path: &Path) -> Result<HashMap<String, Vec<f32>>, String> {
    let mut voices = HashMap::new();
    if path.is_dir() {
        let entries = std::fs::read_dir(path).map_err(|e| format!("read voices dir: {e}"))?;
        for entry in entries.flatten() {
            let file = entry.path();
            if file.extension().and_then(|e| e.to_str()) != Some("bin") {
                continue;
            }
            let Some(stem) = file.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if let Some(data) = read_voice_bin(&file) {
                voices.insert(stem.to_string(), data);
            }
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

/// Read a voice `.bin` as little-endian f32. Returns `None` when the file can't
/// be read or its length isn't a whole number of 256-float style rows.
fn read_voice_bin(path: &Path) -> Option<Vec<f32>> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() || bytes.len() % (STYLE_DIM * 4) != 0 {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect(),
    )
}

/// Convert raw espeak-ng IPA into the Misaki phoneme alphabet for American
/// English, matching the reference `misaki` package's `FROM_ESPEAKS` table.
///
/// The fixed substitutions are applied longest-key-first (as in misaki), then
/// syllabic consonants and length marks are normalized.
fn espeak_ipa_to_misaki(ipa: &str) -> String {
    // espeak's U+0361 tie bar → caret, so multi-letter diphthong keys below match.
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
    fn read_voice_bin_rejects_misaligned() {
        // Not a whole number of 256-float rows → None.
        let path = std::env::temp_dir().join("exoquill_kokoro_bad_voice.bin");
        std::fs::write(&path, [0u8; 7]).unwrap();
        assert!(read_voice_bin(&path).is_none());
        let _ = std::fs::remove_file(&path);
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
}
