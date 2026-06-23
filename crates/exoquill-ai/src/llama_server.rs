//! Persistent llama.cpp server provider for fast, repeated formatting.
//!
//! Unlike [`crate::llama::LlamaFormatter`] (which spawns `llama-cli` per call and
//! reloads the ~1.5B model every time, seconds of latency), this keeps a single
//! `llama-server` child alive with the model resident, so each format is just an
//! HTTP POST to the OpenAI-compatible `/v1/chat/completions` endpoint — cheap
//! enough to run repeatedly when a long note is formatted in chunks. Still an
//! isolated process (decisions D8), now persistent instead of per-call.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::formatter::{FormatRequest, FormatResponse, FormatterProvider};
use crate::llama::DEFAULT_SYSTEM;
use crate::provider::{
    below_normal_priority, free_port, CancelToken, Capability, Health, LicenseInfo,
    ModelRequirement, Provider, ProviderError, ProviderResult,
};

/// A running `llama-server` child + the localhost URL it serves. Dropping it
/// kills the server. Use [`LlamaServer::client`] to talk to it.
pub struct LlamaServer {
    child: Child,
    base_url: String,
}

impl LlamaServer {
    /// Spawn `llama-server` for `model` and wait until the model is loaded.
    /// `-ngl 999` offloads to the GPU when the bundled build is CUDA (ignored on
    /// a CPU build); `-c 4096` is ample for chunk-sized formatting requests.
    ///
    /// On a CPU build the inference is the heavy part, and left unchecked it
    /// saturates every core and starves the webview (the UI freezes). Two guards
    /// keep the app responsive: we cap the worker threads (leaving cores for the
    /// UI/OS, overridable via `EXOQUILL_LLAMA_THREADS`) and, on Windows, drop the
    /// child to below-normal priority so the scheduler favors the foreground UI.
    pub fn start(binary: impl Into<PathBuf>, model: impl Into<PathBuf>) -> ProviderResult<Self> {
        let binary = binary.into();
        let model = model.into();
        let threads = llama_threads();
        // Parallel slots: with `-np N`, llama.cpp serves N requests concurrently
        // via continuous batching, so chunked formatting / speech-prep can run its
        // chunks in parallel instead of one after another. `-c` is the *total*
        // context, split across slots, so it scales with the slot count (4096 per
        // slot). More slots = more KV-cache VRAM; tune via `EXOQUILL_LLAMA_*`.
        let parallel = llama_parallel();
        let context = 4096 * parallel;
        let port = free_port()?;
        let base_url = format!("http://127.0.0.1:{port}");

        let mut command = Command::new(&binary);
        command
            .arg("-m")
            .arg(&model)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("-t")
            .arg(threads.to_string())
            .arg("-np")
            .arg(parallel.to_string())
            .arg("-c")
            .arg(context.to_string())
            .arg("-ngl")
            .arg("999")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        below_normal_priority(&mut command);

        let child = command
            .spawn()
            .map_err(|e| ProviderError::Runtime(format!("spawn llama-server: {e}")))?;

        let server = Self { child, base_url };
        server.wait_ready(Duration::from_secs(60))?;
        Ok(server)
    }

    /// Poll `/health` until the model is loaded (returns 200) or `timeout` passes.
    fn wait_ready(&self, timeout: Duration) -> ProviderResult<()> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|e| ProviderError::Runtime(format!("http client: {e}")))?;
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Ok(resp) = client.get(format!("{}/health", self.base_url)).send() {
                if resp.status().is_success() {
                    return Ok(());
                }
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        Err(ProviderError::Runtime(
            "llama-server did not become ready in time".into(),
        ))
    }

    /// A formatter client bound to this server (own HTTP client; wrap in `Arc`).
    pub fn client(&self) -> ProviderResult<LlamaServerFormatter> {
        LlamaServerFormatter::new(self.base_url.clone())
    }
}

impl Drop for LlamaServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Worker-thread count for llama-server: `EXOQUILL_LLAMA_THREADS` if set, else
/// the logical CPUs minus one (≥1) so the UI/OS keep a core during CPU inference.
fn llama_threads() -> u32 {
    if let Some(n) = std::env::var("EXOQUILL_LLAMA_THREADS")
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .filter(|n| *n > 0)
    {
        return n;
    }
    let cores = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(4);
    cores.saturating_sub(2).max(2)
}

/// Parallel request slots for llama-server: `EXOQUILL_LLAMA_PARALLEL` if set,
/// else 4 — enough to batch a handful of format / speech-prep chunks at once
/// without an outsized KV-cache. Each slot adds context (VRAM); lower it if the
/// GPU is tight (it shares VRAM with the TTS sidecars).
fn llama_parallel() -> u32 {
    std::env::var("EXOQUILL_LLAMA_PARALLEL")
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(4)
}

/// Formatting against a running [`LlamaServer`]'s `/v1/chat/completions` endpoint.
/// The server applies the model's chat template and stops at EOS, so output isn't
/// truncated the way a fixed `-n` cap truncates the per-call CLI.
pub struct LlamaServerFormatter {
    base_url: String,
    client: reqwest::blocking::Client,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    messages: Vec<ChatMessage<'a>>,
    temperature: f32,
    top_p: f32,
    max_tokens: u32,
    stream: bool,
}

/// One streamed SSE delta from `/v1/chat/completions` (`stream: true`).
#[derive(Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

impl LlamaServerFormatter {
    fn new(base_url: String) -> ProviderResult<Self> {
        // A generous total timeout: a long completion may stream for minutes on
        // CPU, so a tight cap would abort mid-generation. The cancel token is the
        // real "stop" — this just bounds a wedged server. (`reqwest::blocking`
        // here has no per-read timeout, so we can't time out on inter-token gaps.)
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(600))
            .build()
            .map_err(|e| ProviderError::Runtime(format!("http client: {e}")))?;
        Ok(Self { base_url, client })
    }

    fn system_prompt(request: &FormatRequest) -> String {
        // Read-aloud prep is a rewrite task, not light formatting — use the
        // instruction as the whole system prompt so the model isn't anchored to
        // the conservative "Diktat/OCR formatter" frame and actually recomposes
        // tables/lists/code into spoken prose.
        if request.operation == "speech_prep" {
            if let Some(instruction) = &request.instruction {
                if !instruction.trim().is_empty() {
                    return instruction.clone();
                }
            }
        }
        match &request.instruction {
            Some(instruction) if !instruction.trim().is_empty() => {
                format!("{DEFAULT_SYSTEM}\n\nZusätzliche Anweisung: {instruction}")
            }
            _ => DEFAULT_SYSTEM.to_string(),
        }
    }
}

impl Provider for LlamaServerFormatter {
    fn id(&self) -> &str {
        "formatter.llama_server"
    }
    fn display_name(&self) -> &str {
        "llama.cpp Formatter (server)"
    }
    fn version(&self) -> &str {
        "1"
    }
    fn capabilities(&self) -> Vec<Capability> {
        Vec::new()
    }
    fn required_models(&self) -> Vec<ModelRequirement> {
        vec![ModelRequirement {
            model_id: "formatter.qwen".into(),
            feature: "formatter".into(),
            required: true,
        }]
    }
    fn license_info(&self) -> LicenseInfo {
        LicenseInfo {
            runtime_license: "MIT".into(),
            source: Some("ggml-org/llama.cpp".into()),
        }
    }
    fn health_check(&self) -> Health {
        match self.client.get(format!("{}/health", self.base_url)).send() {
            Ok(resp) if resp.status().is_success() => Health::Ready,
            Ok(_) => Health::Unavailable {
                reason: "llama-server loading".into(),
            },
            Err(e) => Health::Unavailable {
                reason: format!("llama-server unreachable: {e}"),
            },
        }
    }
}

impl FormatterProvider for LlamaServerFormatter {
    fn run(&self, request: FormatRequest, cancel: &CancelToken) -> ProviderResult<FormatResponse> {
        if cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        if request.text.trim().is_empty() {
            return Ok(FormatResponse {
                formatted_text: String::new(),
                warnings: Vec::new(),
                changed_meaning_risk: "low".into(),
            });
        }

        let system = Self::system_prompt(&request);
        let body = ChatRequest {
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: &system,
                },
                ChatMessage {
                    role: "user",
                    content: request.text.trim(),
                },
            ],
            temperature: 0.3,
            top_p: 0.9,
            max_tokens: 2048,
            stream: true,
        };

        // Stream the completion (SSE) so we can observe `cancel` between tokens
        // and bail mid-generation: returning early drops the response, which
        // closes the connection and makes llama-server stop generating — the
        // freed CPU is what lets a "cancel" actually take effect, instead of the
        // request running to completion in the background.
        let response = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&body)
            .send()
            .map_err(|e| ProviderError::Runtime(format!("llama-server request: {e}")))?;
        if !response.status().is_success() {
            return Err(ProviderError::Runtime(format!(
                "llama-server returned {}",
                response.status()
            )));
        }

        let reader = BufReader::new(response);
        let mut formatted = String::new();
        for line in reader.lines() {
            if cancel.is_cancelled() {
                return Err(ProviderError::Cancelled);
            }
            let line =
                line.map_err(|e| ProviderError::Runtime(format!("llama-server stream: {e}")))?;
            let Some(data) = line.strip_prefix("data:").map(str::trim) else {
                continue;
            };
            if data == "[DONE]" {
                break;
            }
            if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                if let Some(content) = chunk
                    .choices
                    .into_iter()
                    .next()
                    .and_then(|c| c.delta.content)
                {
                    formatted.push_str(&content);
                }
            }
        }

        Ok(FormatResponse {
            formatted_text: formatted.trim().to_string(),
            warnings: Vec::new(),
            changed_meaning_risk: "low".into(),
        })
    }
}
