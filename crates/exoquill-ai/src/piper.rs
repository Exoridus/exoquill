//! Real text-to-speech provider backed by the Piper CLI (sidecar process).
//!
//! Text is written to Piper over stdin; raw 16-bit mono PCM comes back on
//! stdout (`--output-raw`) and is normalized to `f32` samples for playback.
//!
//! A single `PiperTts` instance fronts *all* voices found in a directory: every
//! `*.onnx` next to its `*.onnx.json` config is one selectable voice. The voice
//! is chosen per request via `TtsRequest::voice_id` (the model file stem); the
//! sample rate is read from each voice's config since it varies by quality
//! (`x_low`/`low` are 16 kHz, `medium`/`high` are 22.05 kHz).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::provider::{
    CancelToken, Capability, Health, LicenseInfo, ModelRequirement, Provider, ProviderError,
    ProviderResult,
};
use crate::tts::{TextToSpeechProvider, TtsRequest, TtsResponse, TtsVoice};

/// A discovered Piper voice: its model file plus metadata for the picker.
#[derive(Debug, Clone)]
struct Voice {
    /// Model file stem, e.g. `de_DE-thorsten-medium`. This is the `voice_id`.
    id: String,
    language: String,
    quality: String,
    model: PathBuf,
    sample_rate: u32,
}

/// Neural TTS via a bundled Piper executable + one or more ONNX voices.
pub struct PiperTts {
    binary: PathBuf,
    voices: Vec<Voice>,
    /// Voice id used when a request names an unknown/empty voice. Falls back to
    /// the first discovered voice if this id isn't present.
    default_id: String,
}

impl PiperTts {
    /// Build a provider for a single voice file (the sample rate is taken as
    /// given). Kept for callers/tests that point at one explicit `.onnx`.
    pub fn new(binary: impl Into<PathBuf>, model: impl Into<PathBuf>, sample_rate: u32) -> Self {
        let model = model.into();
        let id = file_stem(&model);
        let (language, quality) = split_voice_id(&id);
        let default_id = id.clone();
        Self {
            binary: binary.into(),
            voices: vec![Voice {
                id,
                language,
                quality,
                model,
                sample_rate,
            }],
            default_id,
        }
    }

    /// Build a provider by scanning `voices_dir` for every `*.onnx` voice. Each
    /// voice's sample rate is read from its sibling `*.onnx.json` (default 22050
    /// if absent/unreadable). `default_id` (a file stem) selects the voice used
    /// for requests that name an unknown voice; if it isn't found, the first
    /// discovered voice wins. Voices are sorted by id for a stable picker order.
    pub fn discover(
        binary: impl Into<PathBuf>,
        voices_dir: impl AsRef<Path>,
        default_id: impl Into<String>,
    ) -> Self {
        let mut voices = Vec::new();
        if let Ok(entries) = std::fs::read_dir(voices_dir.as_ref()) {
            for entry in entries.flatten() {
                let model = entry.path();
                if model.extension().and_then(|e| e.to_str()) != Some("onnx") {
                    continue;
                }
                let id = file_stem(&model);
                if id.is_empty() {
                    continue;
                }
                let (language, quality) = split_voice_id(&id);
                let sample_rate = read_sample_rate(&model).unwrap_or(22_050);
                voices.push(Voice {
                    id,
                    language,
                    quality,
                    model,
                    sample_rate,
                });
            }
        }
        voices.sort_by(|a, b| a.id.cmp(&b.id));
        Self {
            binary: binary.into(),
            voices,
            default_id: default_id.into(),
        }
    }

    /// Pick the voice for a request: the named voice, else the configured
    /// default, else the first discovered voice.
    fn select(&self, voice_id: &str) -> Option<&Voice> {
        self.voices
            .iter()
            .find(|v| v.id == voice_id)
            .or_else(|| self.voices.iter().find(|v| v.id == self.default_id))
            .or_else(|| self.voices.first())
    }
}

impl Provider for PiperTts {
    fn id(&self) -> &str {
        "tts.piper"
    }
    fn display_name(&self) -> &str {
        "Piper TTS"
    }
    fn version(&self) -> &str {
        "1"
    }
    fn capabilities(&self) -> Vec<Capability> {
        Vec::new()
    }
    fn required_models(&self) -> Vec<ModelRequirement> {
        vec![ModelRequirement {
            model_id: "tts.piper.de".into(),
            feature: "tts".into(),
            required: true,
        }]
    }
    fn license_info(&self) -> LicenseInfo {
        LicenseInfo {
            runtime_license: "GPL-3.0".into(),
            source: Some("rhasspy/piper".into()),
        }
    }
    fn health_check(&self) -> Health {
        if self.voices.is_empty() {
            Health::MissingModel {
                model_id: "tts.piper.de".into(),
            }
        } else if self.binary.exists() {
            Health::Ready
        } else {
            Health::Unavailable {
                reason: format!("piper not found at {:?}", self.binary),
            }
        }
    }
}

impl TextToSpeechProvider for PiperTts {
    fn run(&self, request: TtsRequest, cancel: &CancelToken) -> ProviderResult<TtsResponse> {
        if cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        let voice = self
            .select(&request.voice_id)
            .ok_or_else(|| ProviderError::Runtime("no piper voice available".into()))?;

        let text = request.text.trim();
        if text.is_empty() {
            return Ok(TtsResponse {
                samples: Vec::new(),
                sample_rate: voice.sample_rate,
            });
        }

        let mut command = Command::new(&self.binary);
        command.arg("--model").arg(&voice.model).arg("--output_raw");
        // Speaking rate: Piper's length_scale is phoneme *duration*, so a faster
        // voice is a smaller scale — invert. Omit at 1.0 to keep the default.
        if request.speed > 0.0 && (request.speed - 1.0).abs() > f32::EPSILON {
            command
                .arg("--length_scale")
                .arg(format!("{:.3}", 1.0 / request.speed));
        }
        if let Some(noise_scale) = request.expressiveness {
            command
                .arg("--noise_scale")
                .arg(format!("{noise_scale:.3}"));
        }
        if let Some(noise_w) = request.cadence {
            command.arg("--noise_w").arg(format!("{noise_w:.3}"));
        }
        if let Some(silence) = request.sentence_silence {
            command
                .arg("--sentence_silence")
                .arg(format!("{silence:.3}"));
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| ProviderError::Runtime(format!("spawn piper: {e}")))?;

        {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| ProviderError::Runtime("no stdin handle".into()))?;
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| ProviderError::Runtime(format!("write text: {e}")))?;
        } // dropping stdin signals EOF so Piper synthesizes and exits

        let output = child
            .wait_with_output()
            .map_err(|e| ProviderError::Runtime(format!("piper wait: {e}")))?;
        if !output.status.success() {
            return Err(ProviderError::Runtime("piper synthesis failed".into()));
        }

        // Raw 16-bit little-endian mono PCM → normalized f32.
        let samples = output
            .stdout
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
            .collect();
        Ok(TtsResponse {
            samples,
            sample_rate: voice.sample_rate,
        })
    }

    fn voices(&self) -> Vec<TtsVoice> {
        self.voices
            .iter()
            .map(|v| TtsVoice {
                id: v.id.clone(),
                display_name: display_name(v),
                language: v.language.clone(),
                quality: v.quality.clone(),
                provider: "piper".into(),
            })
            .collect()
    }

    fn default_voice(&self) -> Option<String> {
        self.select("").map(|v| v.id.clone())
    }
}

/// File stem ("de_DE-thorsten-medium" for ".../de_DE-thorsten-medium.onnx").
fn file_stem(model: &Path) -> String {
    model
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Split a Piper voice id `<language>-<name>-<quality>` into (language, quality).
/// The name may contain `-`; the first segment is the language and the last is
/// the quality. Ids that don't fit yield empty parts (the id still selects).
fn split_voice_id(id: &str) -> (String, String) {
    let segs: Vec<&str> = id.split('-').collect();
    match segs.as_slice() {
        [lang, _name @ .., qual] if !_name.is_empty() => (lang.to_string(), qual.to_string()),
        [lang, qual] => (lang.to_string(), qual.to_string()),
        _ => (String::new(), String::new()),
    }
}

/// The voice "name" segment(s) between language and quality, title-cased for the
/// picker (`thorsten_emotional` → `Thorsten Emotional`).
fn voice_name(id: &str) -> String {
    let segs: Vec<&str> = id.split('-').collect();
    let name = if segs.len() >= 3 {
        segs[1..segs.len() - 1].join("-")
    } else {
        segs.first().copied().unwrap_or(id).to_string()
    };
    name.split(['_', '-'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Build the picker label, e.g. `Thorsten — de_DE (medium)`.
fn display_name(v: &Voice) -> String {
    let name = voice_name(&v.id);
    match (v.language.is_empty(), v.quality.is_empty()) {
        (false, false) => format!("{name} — {} ({})", v.language, v.quality),
        (false, true) => format!("{name} — {}", v.language),
        (true, false) => format!("{name} ({})", v.quality),
        (true, true) => name,
    }
}

/// Read `audio.sample_rate` from a voice's sibling `*.onnx.json` config.
fn read_sample_rate(model: &Path) -> Option<u32> {
    let config = PathBuf::from(format!("{}.json", model.to_string_lossy()));
    let text = std::fs::read_to_string(config).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    json.get("audio")?
        .get("sample_rate")?
        .as_u64()
        .map(|r| r as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_three_part_id() {
        assert_eq!(
            split_voice_id("de_DE-thorsten-medium"),
            ("de_DE".into(), "medium".into())
        );
    }

    #[test]
    fn splits_name_with_underscore() {
        assert_eq!(
            split_voice_id("de_DE-thorsten_emotional-medium"),
            ("de_DE".into(), "medium".into())
        );
        assert_eq!(
            voice_name("de_DE-thorsten_emotional-medium"),
            "Thorsten Emotional"
        );
    }

    #[test]
    fn builds_label() {
        let v = Voice {
            id: "en_US-amy-medium".into(),
            language: "en_US".into(),
            quality: "medium".into(),
            model: PathBuf::new(),
            sample_rate: 22_050,
        };
        assert_eq!(display_name(&v), "Amy — en_US (medium)");
    }
}
