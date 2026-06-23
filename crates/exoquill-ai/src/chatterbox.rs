//! Chatterbox Multilingual text-to-speech provider (Resemble AI), via a Python sidecar.
//!
//! Like Zonos, Chatterbox is multilingual (incl. German) and clones a voice from a
//! reference `.wav` clip. It outputs 24 kHz mono PCM. A small Python HTTP server
//! (`scripts/chatterbox-server.py`) loads the model once and synthesizes on
//! `POST /tts`; this is a thin blocking client, mirroring [`crate::zonos`].
//!
//! The weights are MIT-licensed (commercial ok). Enable by pointing
//! `EXOQUILL_CHATTERBOX_*` at the venv/script/voice folder; otherwise the other
//! TTS providers are used. Requires a CUDA GPU for practical speed.
//! Note: Chatterbox embeds a Resemble "Perth" watermark in every output.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::provider::{
    below_normal_priority, CancelToken, Capability, Health, LicenseInfo, ModelRequirement,
    Provider, ProviderError, ProviderResult,
};
use crate::tts::{detect_language, TextToSpeechProvider, TtsRequest, TtsResponse, TtsVoice};

/// Chatterbox output is fixed at 24 kHz mono (its native sample rate).
const SAMPLE_RATE: u32 = 24_000;

/// A running Chatterbox Python sidecar child + the localhost URL it serves. Dropping
/// it kills the sidecar. Mirrors [`crate::zonos::ZonosServer`].
pub struct ChatterboxServer {
    child: Child,
    base_url: String,
}

impl ChatterboxServer {
    /// Spawn `python script --port P --voices DIR` and wait until the model is
    /// loaded (the sidecar only answers `GET /` once the model is ready).
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
            .map_err(|e| ProviderError::Runtime(format!("spawn chatterbox sidecar: {e}")))?;
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
            "chatterbox sidecar did not become ready in time".into(),
        ))
    }

    /// A TTS client bound to this sidecar.
    pub fn client(&self) -> Option<ChatterboxTts> {
        ChatterboxTts::connect(self.base_url.clone())
    }
}

impl Drop for ChatterboxServer {
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

/// Thin client for a running Chatterbox sidecar.
pub struct ChatterboxTts {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pitch: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fmax: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    emotion: Option<&'a [f32]>,
}

impl ChatterboxTts {
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

    /// The selectable voices — one per `.wav` reference clip in `dir`.
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
                let stem = path.file_stem()?.to_string_lossy().into_owned();
                Some(TtsVoice {
                    id: stem.clone(),
                    display_name: stem.replace(['_', '-'], " "),
                    language: "auto".into(),
                    quality: "chatterbox".into(),
                    provider: "chatterbox".into(),
                })
            })
            .collect();
        voices.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        voices
    }

    fn parse_speaker(voice_id: &str) -> &str {
        voice_id.trim()
    }
}

impl Provider for ChatterboxTts {
    fn id(&self) -> &str {
        "tts.chatterbox"
    }
    fn display_name(&self) -> &str {
        "Chatterbox Multilingual"
    }
    fn version(&self) -> &str {
        "3"
    }
    fn capabilities(&self) -> Vec<Capability> {
        Vec::new()
    }
    fn required_models(&self) -> Vec<ModelRequirement> {
        vec![ModelRequirement {
            model_id: "tts.chatterbox_v3".into(),
            feature: "tts".into(),
            required: true,
        }]
    }
    fn license_info(&self) -> LicenseInfo {
        LicenseInfo {
            runtime_license: "MIT".into(),
            source: Some("resemble-ai/chatterbox".into()),
        }
    }
    fn health_check(&self) -> Health {
        match self.client.get(&self.base_url).send() {
            Ok(_) => Health::Ready,
            Err(e) => Health::Unavailable {
                reason: format!("chatterbox sidecar unreachable: {e}"),
            },
        }
    }
}

impl TextToSpeechProvider for ChatterboxTts {
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
        let speaker = Self::parse_speaker(&request.voice_id);
        let language = detect_language(text);
        let body = TtsBody {
            text,
            language,
            speaker,
            speed: request.speed,
            pitch: request.intonation,
            fmax: request.brightness,
            emotion: request.emotion.as_deref(),
        };

        let response = self
            .client
            .post(format!("{}/tts", self.base_url))
            .json(&body)
            .send()
            .map_err(|e| ProviderError::Runtime(format!("chatterbox request: {e}")))?;
        if !response.status().is_success() {
            return Err(ProviderError::Runtime(format!(
                "chatterbox sidecar returned {}",
                response.status()
            )));
        }
        let bytes = response
            .bytes()
            .map_err(|e| ProviderError::Runtime(format!("chatterbox read: {e}")))?;

        // Raw 16-bit little-endian mono PCM → normalized f32 (same as Piper/XTTS/Zonos).
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
        None
    }
}
