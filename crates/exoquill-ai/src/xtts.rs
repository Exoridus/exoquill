//! Experimental XTTS-v2 text-to-speech provider (Coqui), via a Python sidecar.
//!
//! Unlike Piper (single-language espeak phonemes), XTTS-v2 is multilingual and
//! handles mixed DE/EN + technical terms far better. It's too heavy to run as a
//! native sidecar, so a small Python HTTP server (`scripts/xtts-server.py`)
//! loads the model once and synthesizes on `POST /tts`; this provider is a thin
//! blocking client, mirroring [`crate::whisper_server`].
//!
//! TEST ONLY: the XTTS-v2 *weights* ship under the non-commercial Coqui Public
//! Model License (CPML). The library (the `coqui-tts` fork) is MPL-2.0 and fine,
//! but the weights must not be redistributed in ExoQuill's GPL build. Enable by
//! running the sidecar and setting `EXOQUILL_XTTS_URL`; otherwise Piper is used.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::provider::{
    below_normal_priority, free_port, probe_health, CancelToken, Capability, Health, LicenseInfo,
    ModelRequirement, Provider, ProviderError, ProviderResult,
};
use crate::tts::{detect_language, TextToSpeechProvider, TtsRequest, TtsResponse, TtsVoice};

/// XTTS-v2 output is fixed at 24 kHz mono.
const SAMPLE_RATE: u32 = 24_000;

/// XTTS-v2's full set of built-in studio speakers. Each is one selectable voice;
/// the speaking language isn't part of the voice — it's auto-detected per segment
/// (see [`detect_language`]) so a speaker reads German or English text optimally
/// without the user picking, and without a de/en duplicate per speaker.
const SPEAKERS: &[&str] = &[
    "Claribel Dervla",
    "Daisy Studious",
    "Gracie Wise",
    "Tammie Ema",
    "Alison Dietlinde",
    "Ana Florence",
    "Annmarie Nele",
    "Asya Anara",
    "Brenda Stern",
    "Gitta Nikolina",
    "Henriette Usha",
    "Sofia Hellen",
    "Tammy Grit",
    "Tanja Adelina",
    "Vjollca Johnnie",
    "Andrew Chipper",
    "Badr Odhiambo",
    "Dionisio Schuyler",
    "Royston Min",
    "Viktor Eka",
    "Abrahan Mack",
    "Adde Michal",
    "Baldur Sanjin",
    "Craig Gutsy",
    "Damien Black",
    "Gilberto Mathias",
    "Ilkin Urabena",
    "Kazuhiko Atallah",
    "Ludvig Milivoj",
    "Suad Qasim",
    "Torcull Diarmuid",
    "Viktor Menelaos",
    "Zacharie Aimilios",
    "Nova Hogarth",
    "Maja Ruoho",
    "Uta Obando",
    "Lidiya Szekeres",
    "Chandra MacFarland",
    "Szofi Granger",
    "Camilla Holmström",
    "Lilya Stainthorpe",
    "Zofija Kendrick",
    "Narelle Moon",
    "Barbora MacLean",
    "Alexandra Hisakawa",
    "Alma María",
    "Rosemary Okafor",
    "Ige Behringer",
    "Filip Traverse",
    "Damjan Chapman",
    "Wulf Carlevaro",
    "Aaron Dreschner",
    "Kumar Dahl",
    "Eugenio Mataracı",
    "Ferran Simen",
    "Xavier Hayasaka",
    "Luis Moray",
    "Marcos Rudaski",
];

/// A running XTTS-v2 Python sidecar child + the localhost URL it serves. Dropping
/// it kills the sidecar. Mirrors [`crate::whisper_server::WhisperServer`] — the
/// app spawns this so XTTS "just works" without the user starting the script.
pub struct XttsServer {
    child: Child,
    base_url: String,
}

impl XttsServer {
    /// Spawn `python script --port P` and wait until the model is loaded (the
    /// sidecar only answers `GET /` once `TTS(...)` finished loading). The model
    /// is normally cached, so loading is seconds; the first ever run downloads
    /// ~1.8 GB, hence the generous timeout.
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
            .map_err(|e| ProviderError::Runtime(format!("spawn xtts sidecar: {e}")))?;
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
            "xtts sidecar did not become ready in time".into(),
        ))
    }

    /// A TTS client bound to this sidecar (own HTTP client; wrap in `Arc`).
    pub fn client(&self) -> Option<XttsTts> {
        XttsTts::connect(self.base_url.clone())
    }
}

impl Drop for XttsServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Thin client for a running XTTS-v2 sidecar.
pub struct XttsTts {
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

impl XttsTts {
    /// Connect to a sidecar at `base_url`; `None` if it isn't reachable. The
    /// synthesis client gets a generous timeout (XTTS is slow on CPU); a short
    /// probe decides reachability.
    pub fn connect(base_url: impl Into<String>) -> Option<Self> {
        let base_url = base_url.into();
        let probe = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .ok()?;
        let resp = probe.get(&base_url).send().ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .ok()?;
        Some(Self { base_url, client })
    }

    /// The selectable voices (one per studio speaker), without needing a live
    /// server — lets the picker populate before the sidecar has finished loading.
    /// Language is auto-detected at synthesis time, so it's not part of the id.
    pub fn voices_static() -> Vec<TtsVoice> {
        SPEAKERS
            .iter()
            .map(|speaker| TtsVoice {
                id: (*speaker).to_string(),
                display_name: (*speaker).to_string(),
                language: "auto".into(),
                quality: "xtts".into(),
                provider: "xtts".into(),
            })
            .collect()
    }

    /// The speaker for a voice id. Accepts a bare speaker name and, for backward
    /// compatibility, the old `"<speaker>|<lang>"` form (the language is ignored
    /// now — it's auto-detected). Falls back to the first speaker.
    fn parse_speaker(voice_id: &str) -> &str {
        let speaker = voice_id.split('|').next().unwrap_or("").trim();
        if speaker.is_empty() {
            SPEAKERS[0]
        } else {
            speaker
        }
    }
}

impl Provider for XttsTts {
    fn id(&self) -> &str {
        "tts.xtts"
    }
    fn display_name(&self) -> &str {
        "XTTS-v2 (experimental)"
    }
    fn version(&self) -> &str {
        "2"
    }
    fn capabilities(&self) -> Vec<Capability> {
        Vec::new()
    }
    fn required_models(&self) -> Vec<ModelRequirement> {
        vec![ModelRequirement {
            model_id: "tts.xtts_v2".into(),
            feature: "tts".into(),
            required: true,
        }]
    }
    fn license_info(&self) -> LicenseInfo {
        LicenseInfo {
            runtime_license: "CPML (non-commercial)".into(),
            source: Some("coqui/XTTS-v2".into()),
        }
    }
    fn health_check(&self) -> Health {
        probe_health(&self.base_url)
    }
}

impl TextToSpeechProvider for XttsTts {
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
        };

        let response = self
            .client
            .post(format!("{}/tts", self.base_url))
            .json(&body)
            .send()
            .map_err(|e| ProviderError::Runtime(format!("xtts request: {e}")))?;
        if !response.status().is_success() {
            return Err(ProviderError::Runtime(format!(
                "xtts sidecar returned {}",
                response.status()
            )));
        }
        let bytes = response
            .bytes()
            .map_err(|e| ProviderError::Runtime(format!("xtts read: {e}")))?;

        // Raw 16-bit little-endian mono PCM → normalized f32 (same as Piper).
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
        Some(SPEAKERS[0].to_string())
    }
}
