//! Qwen3-TTS text-to-speech provider (Alibaba Qwen team), via a Python sidecar.
//!
//! Qwen3-TTS is multilingual (10 languages incl. German) with nine built-in
//! speakers AND voice cloning from a reference clip. A small Python HTTP server
//! (`scripts/qwen3tts-server.py`) loads the model once and synthesizes on
//! `POST /tts`; this is a thin blocking client, mirroring [`crate::chatterbox`].
//!
//! The weights are Apache-2.0 (commercial ok). Enable by pointing
//! `EXOQUILL_QWEN3_*` at the venv/script/voice folder, or install in-app via the
//! model manager. Requires a CUDA GPU for practical speed. Output is resampled to
//! 24 kHz mono by the sidecar.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::provider::{
    below_normal_priority, CancelToken, Capability, Health, LicenseInfo, ModelRequirement,
    Provider, ProviderError, ProviderResult,
};
use crate::tts::{TextToSpeechProvider, TtsRequest, TtsResponse, TtsVoice};

/// The sidecar resamples every output to 24 kHz mono (matches the Rust client).
const SAMPLE_RATE: u32 = 24_000;

/// Qwen3-TTS CustomVoice built-in speakers (no reference clip needed).
const PREDEFINED_SPEAKERS: [&str; 9] = [
    "Vivian", "Serena", "Uncle_Fu", "Dylan", "Eric", "Ryan", "Aiden", "Ono_Anna", "Sohee",
];

/// A running Qwen3 Python sidecar child + the localhost URL it serves. Dropping
/// it kills the sidecar. Mirrors [`crate::chatterbox::ChatterboxServer`].
pub struct Qwen3Server {
    child: Child,
    base_url: String,
}

impl Qwen3Server {
    /// Spawn `python script --port P --voices DIR` and wait until the model is
    /// loaded (the sidecar only answers `GET /` once ready).
    pub fn start(
        python: impl Into<PathBuf>,
        script: impl Into<PathBuf>,
        voices_dir: impl Into<PathBuf>,
    ) -> ProviderResult<Self> {
        let port = free_port()?;
        let base_url = format!("http://127.0.0.1:{port}");
        let mut command = Command::new(python.into());
        command
            .arg(script.into())
            .arg("--port")
            .arg(port.to_string())
            .arg("--voices")
            .arg(voices_dir.into())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        below_normal_priority(&mut command);
        let child = command
            .spawn()
            .map_err(|e| ProviderError::Runtime(format!("spawn qwen3 sidecar: {e}")))?;
        let server = Self { child, base_url };
        server.wait_ready(Duration::from_secs(600))?;
        Ok(server)
    }

    fn wait_ready(&self, timeout: Duration) -> ProviderResult<()> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|e| ProviderError::Runtime(format!("http client: {e}")))?;
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Ok(resp) = client.get(&self.base_url).send() {
                if resp.status().is_success() {
                    return Ok(());
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        Err(ProviderError::Runtime(
            "qwen3 sidecar did not become ready in time".into(),
        ))
    }

    /// A TTS client bound to this sidecar.
    pub fn client(&self) -> Option<Qwen3Tts> {
        Qwen3Tts::connect(self.base_url.clone())
    }
}

impl Drop for Qwen3Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Reserve a free localhost TCP port by binding to :0 and reading it back.
fn free_port() -> ProviderResult<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| ProviderError::Runtime(format!("reserve port: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| ProviderError::Runtime(format!("read port: {e}")))?
        .port();
    Ok(port)
}

/// Thin client for a running Qwen3 sidecar.
pub struct Qwen3Tts {
    base_url: String,
    client: reqwest::blocking::Client,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TtsBody<'a> {
    text: &'a str,
    language: &'a str,
    speaker: &'a str,
    speed: f32,
}

impl Qwen3Tts {
    /// Connect to a sidecar at `base_url`; `None` if it isn't reachable.
    pub fn connect(base_url: impl Into<String>) -> Option<Self> {
        let base_url = base_url.into();
        let probe = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .ok()?;
        probe.get(&base_url).send().ok()?;
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .ok()?;
        Some(Self { base_url, client })
    }

    /// The nine built-in CustomVoice speakers, always available (no clip needed).
    pub fn predefined_voices() -> Vec<TtsVoice> {
        PREDEFINED_SPEAKERS
            .iter()
            .map(|name| TtsVoice {
                id: (*name).to_string(),
                display_name: name.replace('_', " "),
                language: "auto".into(),
                quality: "qwen3".into(),
                provider: "qwen3".into(),
            })
            .collect()
    }

    /// Cloning voices: one per `<name>.wav` that also has a `<name>.txt` transcript
    /// (Qwen3 cloning needs the reference text). Clips without a `.txt` are skipped.
    pub fn voices_in_dir(dir: &Path) -> Vec<TtsVoice> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut voices: Vec<TtsVoice> = entries
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                let is_wav = path
                    .extension()
                    .and_then(|x| x.to_str())
                    .is_some_and(|x| x.eq_ignore_ascii_case("wav"));
                if !is_wav {
                    return None;
                }
                if !path.with_extension("txt").exists() {
                    return None; // no transcript → not usable for Qwen3 cloning
                }
                let stem = path.file_stem()?.to_string_lossy().into_owned();
                Some(TtsVoice {
                    id: stem.clone(),
                    display_name: stem.replace(['_', '-'], " "),
                    language: "auto".into(),
                    quality: "qwen3-clone".into(),
                    provider: "qwen3".into(),
                })
            })
            .collect();
        voices.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        voices
    }
}

impl Provider for Qwen3Tts {
    fn id(&self) -> &str {
        "tts.qwen3"
    }
    fn display_name(&self) -> &str {
        "Qwen3-TTS"
    }
    fn version(&self) -> &str {
        "3"
    }
    fn capabilities(&self) -> Vec<Capability> {
        Vec::new()
    }
    fn required_models(&self) -> Vec<ModelRequirement> {
        vec![ModelRequirement {
            model_id: "tts.qwen3".into(),
            feature: "tts".into(),
            required: true,
        }]
    }
    fn license_info(&self) -> LicenseInfo {
        LicenseInfo {
            runtime_license: "Apache-2.0".into(),
            source: Some("QwenLM/Qwen3-TTS".into()),
        }
    }
    fn health_check(&self) -> Health {
        match self.client.get(&self.base_url).send() {
            Ok(_) => Health::Ready,
            Err(e) => Health::Unavailable {
                reason: format!("qwen3 sidecar unreachable: {e}"),
            },
        }
    }
}

impl TextToSpeechProvider for Qwen3Tts {
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
        let body = TtsBody {
            text,
            language: "Auto",
            speaker: request.voice_id.trim(),
            speed: request.speed,
        };

        let response = self
            .client
            .post(format!("{}/tts", self.base_url))
            .json(&body)
            .send()
            .map_err(|e| ProviderError::Runtime(format!("qwen3 request: {e}")))?;
        if !response.status().is_success() {
            return Err(ProviderError::Runtime(format!(
                "qwen3 sidecar returned {}",
                response.status()
            )));
        }
        let bytes = response
            .bytes()
            .map_err(|e| ProviderError::Runtime(format!("qwen3 read: {e}")))?;

        let samples = bytes
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
            .collect();
        Ok(TtsResponse {
            samples,
            sample_rate: SAMPLE_RATE,
        })
    }

    fn voices(&self) -> Vec<TtsVoice> {
        Vec::new()
    }

    fn default_voice(&self) -> Option<String> {
        Some(PREDEFINED_SPEAKERS[0].to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predefined_voices_has_nine_known_speakers() {
        let voices = Qwen3Tts::predefined_voices();
        assert_eq!(voices.len(), 9);
        assert!(voices.iter().all(|v| v.provider == "qwen3"));
        assert!(voices.iter().any(|v| v.id == "Vivian"));
        assert!(voices.iter().any(|v| v.id == "Ono_Anna"));
        // display name de-underscores
        let anna = voices.iter().find(|v| v.id == "Ono_Anna").unwrap();
        assert_eq!(anna.display_name, "Ono Anna");
    }

    #[test]
    fn voices_in_dir_requires_a_transcript() {
        let dir = std::env::temp_dir().join(format!("qwen3_voices_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // clip WITH transcript → counts
        std::fs::write(dir.join("dima.wav"), b"RIFFxxxx").unwrap();
        std::fs::write(dir.join("dima.txt"), "hallo welt").unwrap();
        // clip WITHOUT transcript → skipped
        std::fs::write(dir.join("nope.wav"), b"RIFFxxxx").unwrap();
        // non-wav → ignored
        std::fs::write(dir.join("readme.md"), b"x").unwrap();

        let voices = Qwen3Tts::voices_in_dir(&dir);
        std::fs::remove_dir_all(&dir).ok();

        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].id, "dima");
        assert_eq!(voices[0].provider, "qwen3");
        assert_eq!(voices[0].quality, "qwen3-clone");
    }
}
