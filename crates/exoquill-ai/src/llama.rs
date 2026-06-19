//! Real formatter provider backed by the llama.cpp CLI (sidecar process).
//!
//! Builds a ChatML prompt for a Qwen instruct model and runs a single-shot
//! completion via `llama-cli`. The model is expected to clean up rough
//! dictation/OCR text without inventing content (product spec §12).

use std::path::PathBuf;
use std::process::Command;

use crate::formatter::{FormatRequest, FormatResponse, FormatterProvider};
use crate::provider::{
    CancelToken, Capability, Health, LicenseInfo, ModelRequirement, Provider, ProviderError,
    ProviderResult,
};

const DEFAULT_SYSTEM: &str = "Du bist ein präziser Text-Formatierer für Diktate und OCR-Text. \
Korrigiere Rechtschreibung, Zeichensetzung und offensichtliche Erkennungsfehler und verbessere \
die Lesbarkeit mit sauberem Markdown. Erfinde keine neuen Inhalte, bewahre Bedeutung und \
Fachbegriffe (Produkt- und Bibliotheksnamen). Gib ausschließlich den formatierten Text zurück.";

/// Text formatting via a bundled llama.cpp executable + Qwen GGUF model.
pub struct LlamaFormatter {
    binary: PathBuf,
    model: PathBuf,
    threads: u32,
    max_tokens: u32,
}

impl LlamaFormatter {
    pub fn new(binary: impl Into<PathBuf>, model: impl Into<PathBuf>) -> Self {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(4);
        Self {
            binary: binary.into(),
            model: model.into(),
            threads,
            max_tokens: 1024,
        }
    }

    fn build_prompt(request: &FormatRequest) -> String {
        let system = match &request.instruction {
            Some(instruction) if !instruction.trim().is_empty() => {
                format!("{DEFAULT_SYSTEM}\n\nZusätzliche Anweisung: {instruction}")
            }
            _ => DEFAULT_SYSTEM.to_string(),
        };
        format!(
            "<|im_start|>system\n{system}<|im_end|>\n\
             <|im_start|>user\n{}<|im_end|>\n\
             <|im_start|>assistant\n",
            request.text.trim()
        )
    }

    /// Strip ANSI escape sequences (llama-completion colorizes its output).
    fn strip_ansi(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    fn clean_output(raw: &str) -> String {
        let text = Self::strip_ansi(raw);
        // llama-completion drops into an interactive prompt at the end
        // ("> EOF by user") once stdin closes; keep only what's before it.
        let body = text.split("EOF by user").next().unwrap_or(&text);
        body.trim_end_matches(['>', ' ', '\r', '\n', '\t'])
            .trim_end_matches("<|im_end|>")
            .trim()
            .to_string()
    }
}

impl Provider for LlamaFormatter {
    fn id(&self) -> &str {
        "formatter.llamacpp"
    }
    fn display_name(&self) -> &str {
        "llama.cpp Formatter"
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
        if !self.model.exists() {
            return Health::MissingModel {
                model_id: "formatter.qwen".into(),
            };
        }
        match Command::new(&self.binary).arg("--version").output() {
            Ok(out) if out.status.success() => Health::Ready,
            _ => Health::Unavailable {
                reason: format!("llama-cli not runnable at {:?}", self.binary),
            },
        }
    }
}

impl FormatterProvider for LlamaFormatter {
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

        let prompt = Self::build_prompt(&request);
        let output = Command::new(&self.binary)
            .arg("-m")
            .arg(&self.model)
            .arg("-p")
            .arg(&prompt)
            .arg("-n")
            .arg(self.max_tokens.to_string())
            .arg("-t")
            .arg(self.threads.to_string())
            .arg("--no-display-prompt")
            .arg("--no-perf")
            .arg("--temp")
            .arg("0.3")
            .arg("--top-p")
            .arg("0.9")
            .output()
            .map_err(|e| ProviderError::Runtime(format!("spawn llama-cli: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ProviderError::Runtime(format!(
                "llama-cli failed: {}",
                stderr.lines().last().unwrap_or("unknown error")
            )));
        }

        let formatted = Self::clean_output(&String::from_utf8_lossy(&output.stdout));
        Ok(FormatResponse {
            formatted_text: formatted,
            warnings: Vec::new(),
            changed_meaning_risk: "low".into(),
        })
    }
}
