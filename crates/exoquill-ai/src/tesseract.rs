//! Real OCR provider backed by the Tesseract CLI (sidecar process).
//!
//! Images are preprocessed via `exoquill-capture` and piped to `tesseract`
//! over stdin (`tesseract - stdout`), so no temp files are needed.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::ocr::{OcrProvider, OcrRequest, OcrResponse};
use crate::provider::{
    CancelToken, Capability, Health, LicenseInfo, ModelRequirement, Provider, ProviderError,
    ProviderResult,
};

/// OCR via a bundled Tesseract executable.
pub struct TesseractOcr {
    /// Path to the `tesseract` executable.
    binary: PathBuf,
    /// Directory holding the `*.traineddata` files (`--tessdata-dir`). When
    /// `None`, Tesseract uses its default `TESSDATA_PREFIX`.
    tessdata_dir: Option<PathBuf>,
}

impl TesseractOcr {
    pub fn new(binary: impl Into<PathBuf>, tessdata_dir: Option<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
            tessdata_dir,
        }
    }
}

impl Provider for TesseractOcr {
    fn id(&self) -> &str {
        "ocr.tesseract"
    }
    fn display_name(&self) -> &str {
        "Tesseract OCR"
    }
    fn version(&self) -> &str {
        "5"
    }
    fn capabilities(&self) -> Vec<Capability> {
        vec![Capability {
            key: "languages".into(),
            description: "deu, eng".into(),
        }]
    }
    fn required_models(&self) -> Vec<ModelRequirement> {
        vec![ModelRequirement {
            model_id: "tessdata.deu+eng".into(),
            feature: "ocr".into(),
            required: true,
        }]
    }
    fn license_info(&self) -> LicenseInfo {
        LicenseInfo {
            runtime_license: "Apache-2.0".into(),
            source: Some("tesseract-ocr/tesseract".into()),
        }
    }
    fn health_check(&self) -> Health {
        match Command::new(&self.binary).arg("--version").output() {
            Ok(out) if out.status.success() => Health::Ready,
            _ => Health::Unavailable {
                reason: format!("tesseract not runnable at {:?}", self.binary),
            },
        }
    }
}

impl OcrProvider for TesseractOcr {
    fn run(&self, request: OcrRequest, cancel: &CancelToken) -> ProviderResult<OcrResponse> {
        if cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        if request.image_bytes.is_empty() {
            return Err(ProviderError::InvalidInput("empty image".into()));
        }

        // Clean up the image first for better recognition.
        let image = exoquill_capture::preprocess_for_ocr(&request.image_bytes)
            .map_err(|e| ProviderError::InvalidInput(format!("preprocess: {e}")))?;

        let languages = if request.languages.is_empty() {
            "deu+eng".to_string()
        } else {
            request.languages.clone()
        };

        let mut command = Command::new(&self.binary);
        command
            .arg("-") // read image from stdin
            .arg("stdout") // write text to stdout
            .arg("-l")
            .arg(&languages);
        if let Some(dir) = &self.tessdata_dir {
            command.arg("--tessdata-dir").arg(dir);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command
            .spawn()
            .map_err(|e| ProviderError::Runtime(format!("spawn tesseract: {e}")))?;
        child
            .stdin
            .take()
            .ok_or_else(|| ProviderError::Runtime("no stdin handle".into()))?
            .write_all(&image)
            .map_err(|e| ProviderError::Runtime(format!("write image: {e}")))?;

        if cancel.is_cancelled() {
            let _ = child.kill();
            return Err(ProviderError::Cancelled);
        }

        let output = child
            .wait_with_output()
            .map_err(|e| ProviderError::Runtime(format!("tesseract wait: {e}")))?;
        if !output.status.success() {
            return Err(ProviderError::Runtime(format!(
                "tesseract failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }

        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(OcrResponse {
            text,
            confidence: None,
        })
    }
}
