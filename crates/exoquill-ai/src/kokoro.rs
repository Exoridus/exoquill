//! Kokoro-82M text-to-speech provider (hexgrad/Kokoro), via a Python sidecar.
//!
//! Kokoro is a lightweight (82 M parameter), Apache-2.0 English TTS model that
//! runs on CPU without a GPU. It outputs 24 kHz mono PCM. A small Python HTTP
//! server (`scripts/kokoro-server.py`) loads the model once and synthesizes on
//! `POST /tts`; this is a thin blocking client, mirroring [`crate::chatterbox`].
//!
//! Unlike Chatterbox/Zonos, Kokoro has a FIXED set of built-in voices — no
//! reference `.wav` clips are needed. Enable by pointing
//! `EXOQUILL_KOKORO_PYTHON` at the venv python and letting ExoQuill resolve the
//! script path automatically; otherwise the other TTS providers are used.

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::provider::{
    below_normal_priority, CancelToken, Capability, Health, LicenseInfo, ModelRequirement,
    Provider, ProviderError, ProviderResult,
};
use crate::tts::{TextToSpeechProvider, TtsRequest, TtsResponse, TtsVoice};

/// Kokoro output is fixed at 24 kHz mono (its native sample rate).
const SAMPLE_RATE: u32 = 24_000;

/// Kokoro-82M's built-in voice set. These IDs match the `kokoro` pip package's
/// voice names. More voices exist but this covers the practical default set.
const VOICES: &[(&str, &str)] = &[
    ("af_heart", "Heart (AF)"),
    ("af_bella", "Bella (AF)"),
    ("am_michael", "Michael (AM)"),
    ("bf_emma", "Emma (BF)"),
    ("bm_george", "George (BM)"),
];

/// A running Kokoro Python sidecar child + the localhost URL it serves. Dropping
/// it kills the sidecar. Mirrors [`crate::chatterbox::ChatterboxServer`].
pub struct KokoroServer {
    child: Child,
    base_url: String,
}

impl KokoroServer {
    /// Spawn `python script --port P` and wait until the model is loaded (the
    /// sidecar only answers `GET /` once the model is ready).
    pub fn start(python: impl Into<PathBuf>, script: impl Into<PathBuf>) -> ProviderResult<Self> {
        let port = free_port()?;
        let base_url = format!("http://127.0.0.1:{port}");
        let mut command = Command::new(python.into());
        command
            .arg(script.into())
            .arg("--port")
            .arg(port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        below_normal_priority(&mut command);
        let child = command
            .spawn()
            .map_err(|e| ProviderError::Runtime(format!("spawn kokoro sidecar: {e}")))?;
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
            "kokoro sidecar did not become ready in time".into(),
        ))
    }

    /// A TTS client bound to this sidecar.
    pub fn client(&self) -> Option<KokoroTts> {
        KokoroTts::connect(self.base_url.clone())
    }
}

impl Drop for KokoroServer {
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

/// Thin client for a running Kokoro sidecar.
pub struct KokoroTts {
    base_url: String,
    client: reqwest::blocking::Client,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TtsBody<'a> {
    text: &'a str,
    voice: &'a str,
    speed: f32,
}

impl KokoroTts {
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

    /// The selectable voices — fixed built-in set, available without a live
    /// server so the picker populates before the sidecar has finished loading.
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

    /// The voice id to send. Falls back to the first built-in voice.
    fn parse_voice(voice_id: &str) -> &str {
        let v = voice_id.trim();
        if v.is_empty() {
            VOICES[0].0
        } else {
            v
        }
    }
}

impl Provider for KokoroTts {
    fn id(&self) -> &str {
        "tts.kokoro"
    }
    fn display_name(&self) -> &str {
        "Kokoro-82M"
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
        match self.client.get(&self.base_url).send() {
            Ok(_) => Health::Ready,
            Err(e) => Health::Unavailable {
                reason: format!("kokoro sidecar unreachable: {e}"),
            },
        }
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
        let voice = Self::parse_voice(&request.voice_id);
        let body = TtsBody {
            text,
            voice,
            speed: request.speed,
        };

        let response = self
            .client
            .post(format!("{}/tts", self.base_url))
            .json(&body)
            .send()
            .map_err(|e| ProviderError::Runtime(format!("kokoro request: {e}")))?;
        if !response.status().is_success() {
            return Err(ProviderError::Runtime(format!(
                "kokoro sidecar returned {}",
                response.status()
            )));
        }
        let bytes = response
            .bytes()
            .map_err(|e| ProviderError::Runtime(format!("kokoro read: {e}")))?;

        // Raw 16-bit little-endian mono PCM → normalized f32 (same as Piper/XTTS/Zonos/Chatterbox).
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
        Self::voices_static()
    }

    fn default_voice(&self) -> Option<String> {
        Some(VOICES[0].0.to_string())
    }
}
