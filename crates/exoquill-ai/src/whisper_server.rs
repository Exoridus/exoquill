//! Persistent whisper.cpp server provider for low-latency, streaming dictation.
//!
//! Unlike [`crate::whisper::WhisperStt`] (which spawns `whisper-cli` per call and
//! reloads the model every time, ~600 ms), this keeps a single `whisper-server`
//! child alive with the model resident on the GPU, so each transcription is just
//! an HTTP POST to `/inference` — cheap enough to run repeatedly on the
//! in-progress utterance for live partial transcripts. Still an isolated process
//! (decisions D8), now persistent instead of per-call.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::provider::{
    free_port, CancelToken, Capability, Health, LicenseInfo, ModelRequirement, Provider,
    ProviderError, ProviderResult,
};
use crate::stt::{SpeechToTextProvider, SttRequest, SttResponse};
use crate::whisper::WhisperStt;

/// A running `whisper-server` child + the localhost URL it serves. Dropping it
/// kills the server. Use [`WhisperServer::client`] to talk to it.
pub struct WhisperServer {
    child: Child,
    base_url: String,
}

impl WhisperServer {
    /// Spawn `whisper-server` for `model` and wait until it answers. `binary` is
    /// the `whisper-server` executable; its ggml backend DLLs must sit next to it
    /// (the build script puts them in the same `runtimes/whisper/` dir), so it
    /// picks the GPU automatically like `whisper-cli`.
    pub fn start(binary: impl Into<PathBuf>, model: impl Into<PathBuf>) -> ProviderResult<Self> {
        let binary = binary.into();
        let model = model.into();
        let threads = std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(4);
        let port = free_port()?;
        let base_url = format!("http://127.0.0.1:{port}");

        let child = Command::new(&binary)
            .arg("-m")
            .arg(&model)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("-t")
            .arg(threads.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| ProviderError::Runtime(format!("spawn whisper-server: {e}")))?;

        let server = Self { child, base_url };
        server.wait_ready(Duration::from_secs(40))?;
        Ok(server)
    }

    /// Poll the server until it responds or `timeout` elapses (model load + GPU
    /// init can take a few seconds on the first start).
    fn wait_ready(&self, timeout: Duration) -> ProviderResult<()> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|e| ProviderError::Runtime(format!("http client: {e}")))?;
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if client.get(&self.base_url).send().is_ok() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        Err(ProviderError::Runtime(
            "whisper-server did not become ready in time".into(),
        ))
    }

    /// A cheap STT client bound to this server (own HTTP client; wrap in `Arc`).
    pub fn client(&self) -> ProviderResult<WhisperServerStt> {
        WhisperServerStt::new(self.base_url.clone())
    }
}

impl Drop for WhisperServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Speech-to-text against a running [`WhisperServer`]'s `/inference` endpoint.
pub struct WhisperServerStt {
    base_url: String,
    client: reqwest::blocking::Client,
}

#[derive(Deserialize)]
struct InferenceResponse {
    text: String,
}

impl WhisperServerStt {
    fn new(base_url: String) -> ProviderResult<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| ProviderError::Runtime(format!("http client: {e}")))?;
        Ok(Self { base_url, client })
    }
}

impl Provider for WhisperServerStt {
    fn id(&self) -> &str {
        "stt.whisper_server"
    }
    fn display_name(&self) -> &str {
        "Whisper STT (server)"
    }
    fn version(&self) -> &str {
        "1"
    }
    fn capabilities(&self) -> Vec<Capability> {
        Vec::new()
    }
    fn required_models(&self) -> Vec<ModelRequirement> {
        vec![ModelRequirement {
            model_id: "stt.whisper".into(),
            feature: "stt".into(),
            required: true,
        }]
    }
    fn license_info(&self) -> LicenseInfo {
        LicenseInfo {
            runtime_license: "MIT".into(),
            source: Some("ggml-org/whisper.cpp".into()),
        }
    }
    fn health_check(&self) -> Health {
        match self.client.get(&self.base_url).send() {
            Ok(_) => Health::Ready,
            Err(e) => Health::Unavailable {
                reason: format!("whisper-server unreachable: {e}"),
            },
        }
    }
}

impl SpeechToTextProvider for WhisperServerStt {
    fn run(&self, request: SttRequest, cancel: &CancelToken) -> ProviderResult<SttResponse> {
        if cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        let language = WhisperStt::language_flag(&request.language_mode);
        if request.samples.is_empty() {
            return Ok(SttResponse {
                text: String::new(),
                language: Some(language.into()),
                confidence: None,
            });
        }

        let wav = WhisperStt::encode_wav(&request.samples, request.sample_rate);
        let part = reqwest::blocking::multipart::Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| ProviderError::Runtime(format!("multipart part: {e}")))?;
        let mut form = reqwest::blocking::multipart::Form::new()
            .part("file", part)
            .text("response_format", "json")
            .text("temperature", "0")
            .text("language", language);
        if let Some(prompt) = WhisperStt::build_prompt(&request.custom_terms) {
            form = form.text("prompt", prompt);
        }

        let response = self
            .client
            .post(format!("{}/inference", self.base_url))
            .multipart(form)
            .send()
            .map_err(|e| ProviderError::Runtime(format!("whisper-server request: {e}")))?;
        if !response.status().is_success() {
            return Err(ProviderError::Runtime(format!(
                "whisper-server returned {}",
                response.status()
            )));
        }
        let parsed: InferenceResponse = response
            .json()
            .map_err(|e| ProviderError::Runtime(format!("parse whisper-server response: {e}")))?;

        Ok(SttResponse {
            text: WhisperStt::parse_transcript(&parsed.text),
            language: Some(language.into()),
            confidence: None,
        })
    }
}
