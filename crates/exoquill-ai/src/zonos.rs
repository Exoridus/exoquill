//! Zonos-v0.1 text-to-speech provider (Zyphra), via a Python sidecar.
//!
//! Like XTTS, Zonos is multilingual (incl. German) and far better with mixed
//! DE/EN technical terms than Piper. Unlike XTTS it has no fixed studio speakers:
//! it *clones* a voice from a 10–30 s reference clip. So a voice here is a `.wav`
//! in a reference folder (mirrors how `PiperTts` treats a folder of `.onnx`),
//! and the sidecar embeds that clip to synthesize. It's too heavy to run as a
//! native sidecar, so a small Python HTTP server (`scripts/zonos-server.py`)
//! loads the model once and synthesizes on `POST /tts`; this is a thin blocking
//! client, mirroring [`crate::xtts`].
//!
//! Unlike XTTS, the Zonos *weights* are Apache-2.0 — fine to redistribute — but
//! the model needs a CUDA GPU to be usable (CPU is far too slow). Enable by
//! pointing `EXOQUILL_ZONOS_*` at the venv/script/voice folder; otherwise the
//! other TTS providers are used.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::provider::{
    below_normal_priority, CancelToken, Capability, Health, LicenseInfo, ModelRequirement, Provider,
    ProviderError, ProviderResult,
};
use crate::tts::{detect_language, TextToSpeechProvider, TtsRequest, TtsResponse, TtsVoice};

/// Zonos output is fixed at 44.1 kHz mono (its DAC autoencoder's native rate).
const SAMPLE_RATE: u32 = 44_100;

/// A running Zonos Python sidecar child + the localhost URL it serves. Dropping
/// it kills the sidecar. Mirrors [`crate::xtts::XttsServer`].
pub struct ZonosServer {
    child: Child,
    base_url: String,
}

impl ZonosServer {
    /// Spawn `python script --port P --voices DIR` and wait until the model is
    /// loaded (the sidecar only answers `GET /` once the model is ready). The
    /// model is cached after the first run, so loading is seconds; the first ever
    /// run downloads the weights, hence the generous timeout.
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
            .map_err(|e| ProviderError::Runtime(format!("spawn zonos sidecar: {e}")))?;
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
            "zonos sidecar did not become ready in time".into(),
        ))
    }

    /// A TTS client bound to this sidecar (own HTTP client; wrap in `Arc`).
    pub fn client(&self) -> Option<ZonosTts> {
        ZonosTts::connect(self.base_url.clone())
    }
}

impl Drop for ZonosServer {
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

/// Thin client for a running Zonos sidecar.
pub struct ZonosTts {
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
    /// `pitch_std` — intonation liveliness; `None` lets the sidecar use its
    /// default. Maps from [`TtsRequest::intonation`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pitch: Option<f32>,
    /// `fmax` — frequency ceiling (Hz); `None` keeps the sidecar default of
    /// 22050. Maps from [`TtsRequest::brightness`].
    #[serde(skip_serializing_if = "Option::is_none")]
    fmax: Option<f32>,
    /// `emotion` — the 8-value conditioning vector; `None` lets the sidecar leave
    /// Zonos' default. Maps from [`TtsRequest::emotion`].
    #[serde(skip_serializing_if = "Option::is_none")]
    emotion: Option<&'a [f32]>,
}

impl ZonosTts {
    /// Connect to a sidecar at `base_url`; `None` if it isn't reachable. The
    /// synthesis client gets a generous timeout (neural TTS isn't instant); a
    /// short probe decides reachability.
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

    /// The selectable voices — one per `.wav` reference clip in `dir` (the file
    /// stem is the voice id), without needing a live server, so the picker can
    /// populate before the sidecar has finished loading. Empty when the folder is
    /// missing or has no clips. Mirrors how `PiperTts` enumerates a voice folder.
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
                    quality: "zonos".into(),
                    provider: "zonos".into(),
                })
            })
            .collect();
        voices.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        voices
    }

    /// The reference clip (voice id) for a request; empty falls back to the
    /// sidecar's own default (it picks the first clip it loaded).
    fn parse_speaker(voice_id: &str) -> &str {
        voice_id.trim()
    }
}

impl Provider for ZonosTts {
    fn id(&self) -> &str {
        "tts.zonos"
    }
    fn display_name(&self) -> &str {
        "Zonos-v0.1"
    }
    fn version(&self) -> &str {
        "0.1"
    }
    fn capabilities(&self) -> Vec<Capability> {
        Vec::new()
    }
    fn required_models(&self) -> Vec<ModelRequirement> {
        vec![ModelRequirement {
            model_id: "tts.zonos_v0_1".into(),
            feature: "tts".into(),
            required: true,
        }]
    }
    fn license_info(&self) -> LicenseInfo {
        LicenseInfo {
            runtime_license: "Apache-2.0".into(),
            source: Some("Zyphra/Zonos-v0.1".into()),
        }
    }
    fn health_check(&self) -> Health {
        match self.client.get(&self.base_url).send() {
            Ok(_) => Health::Ready,
            Err(e) => Health::Unavailable {
                reason: format!("zonos sidecar unreachable: {e}"),
            },
        }
    }
}

impl TextToSpeechProvider for ZonosTts {
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
            .map_err(|e| ProviderError::Runtime(format!("zonos request: {e}")))?;
        if !response.status().is_success() {
            return Err(ProviderError::Runtime(format!(
                "zonos sidecar returned {}",
                response.status()
            )));
        }
        let bytes = response
            .bytes()
            .map_err(|e| ProviderError::Runtime(format!("zonos read: {e}")))?;

        // Raw 16-bit little-endian mono PCM → normalized f32 (same as Piper/XTTS).
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
        // The live client doesn't own the folder; voices are listed statically by
        // the command layer via `voices_in_dir`. Empty here is fine.
        Vec::new()
    }

    fn default_voice(&self) -> Option<String> {
        None
    }
}
