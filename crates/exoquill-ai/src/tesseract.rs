//! Real OCR provider backed by the Tesseract CLI (sidecar process).
//!
//! Images are preprocessed via `exoquill-capture` and piped to `tesseract`
//! over stdin (`tesseract - stdout`), so no temp files are needed.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::ocr::{OcrLayout, OcrProvider, OcrRequest, OcrResponse, OcrWord};
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

    /// Preprocess `image_bytes`, run Tesseract over stdin and return its stdout.
    /// With `tsv`, the TSV renderer is enabled (word boxes) via a config
    /// variable — not a config *file*, which the bundled tessdata omits.
    /// Shared by [`run`](OcrProvider::run) and [`run_layout`](OcrProvider::run_layout).
    fn invoke(
        &self,
        image_bytes: &[u8],
        languages: &str,
        tsv: bool,
        cancel: &CancelToken,
    ) -> ProviderResult<Vec<u8>> {
        if cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        if image_bytes.is_empty() {
            return Err(ProviderError::InvalidInput("empty image".into()));
        }

        // Clean up the image first for better recognition (decode → grayscale →
        // upscale 2x → contrast); word boxes are returned in this 2x space.
        let image = exoquill_capture::preprocess_for_ocr(image_bytes)
            .map_err(|e| ProviderError::InvalidInput(format!("preprocess: {e}")))?;

        let mut command = Command::new(&self.binary);
        command
            .arg("-") // read image from stdin
            .arg("stdout") // write to stdout
            .arg("-l")
            .arg(languages);
        if let Some(dir) = &self.tessdata_dir {
            command.arg("--tessdata-dir").arg(dir);
        }
        if tsv {
            command.arg("-c").arg("tessedit_create_tsv=1");
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
        Ok(output.stdout)
    }

    fn languages(request: &OcrRequest) -> String {
        if request.languages.is_empty() {
            "deu+eng".to_string()
        } else {
            request.languages.clone()
        }
    }

    /// Parse Tesseract TSV (one row per layout element) into an [`OcrLayout`]:
    /// word boxes plus text reconstructed with the page's line and paragraph
    /// breaks, and the page pixel dimensions (from the `level 1` row).
    fn parse_tsv(tsv: &str) -> OcrLayout {
        let mut words = Vec::new();
        let mut text = String::new();
        let (mut width, mut height) = (0u32, 0u32);
        let (mut last_block, mut last_par, mut last_line) = (-1i64, -1i64, -1i64);

        for row in tsv.lines().skip(1) {
            let f: Vec<&str> = row.split('\t').collect();
            if f.len() < 12 {
                continue;
            }
            let level: i64 = f[0].parse().unwrap_or(0);
            if level == 1 {
                width = f[8].parse().unwrap_or(0);
                height = f[9].parse().unwrap_or(0);
                continue;
            }
            if level != 5 {
                continue;
            }
            let word = f[11].trim();
            let conf: f32 = f[10].parse().unwrap_or(-1.0);
            if word.is_empty() || conf < 0.0 {
                continue;
            }
            let (block, par, line) = (
                f[2].parse().unwrap_or(0),
                f[3].parse().unwrap_or(0),
                f[4].parse().unwrap_or(0),
            );
            if !words.is_empty() {
                if block != last_block || par != last_par {
                    text.push_str("\n\n");
                } else if line != last_line {
                    text.push('\n');
                } else {
                    text.push(' ');
                }
            }
            text.push_str(word);
            (last_block, last_par, last_line) = (block, par, line);
            words.push(OcrWord {
                text: word.to_string(),
                x: f[6].parse().unwrap_or(0),
                y: f[7].parse().unwrap_or(0),
                width: f[8].parse().unwrap_or(0),
                height: f[9].parse().unwrap_or(0),
                confidence: conf,
            });
        }
        OcrLayout {
            text,
            words,
            width,
            height,
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
        let stdout = self.invoke(
            &request.image_bytes,
            &Self::languages(&request),
            false,
            cancel,
        )?;
        Ok(OcrResponse {
            text: String::from_utf8_lossy(&stdout).trim().to_string(),
            confidence: None,
        })
    }

    fn run_layout(&self, request: OcrRequest, cancel: &CancelToken) -> ProviderResult<OcrLayout> {
        let stdout = self.invoke(
            &request.image_bytes,
            &Self::languages(&request),
            true,
            cancel,
        )?;
        Ok(Self::parse_tsv(&String::from_utf8_lossy(&stdout)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tsv_reconstructs_layout_and_boxes() {
        // Minimal Tesseract-style TSV: a page, then two lines in one block/par.
        let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
            1\t1\t0\t0\t0\t0\t0\t0\t640\t480\t-1\t\n\
            5\t1\t1\t1\t1\t1\t10\t12\t40\t20\t96\tHallo\n\
            5\t1\t1\t1\t1\t2\t60\t12\t40\t20\t95\tWelt\n\
            5\t1\t1\t1\t2\t1\t10\t40\t50\t20\t90\tZeile\n\
            5\t1\t2\t1\t1\t1\t10\t90\t60\t20\t88\tAbsatz";
        let layout = TesseractOcr::parse_tsv(tsv);
        assert_eq!(layout.width, 640);
        assert_eq!(layout.height, 480);
        assert_eq!(layout.words.len(), 4);
        assert_eq!(layout.text, "Hallo Welt\nZeile\n\nAbsatz");
        assert_eq!(layout.words[0].text, "Hallo");
        assert_eq!(layout.words[0].x, 10);
        assert_eq!(layout.words[0].height, 20);
    }

    /// Real end-to-end check against a bundled Tesseract + image. Ignored by
    /// default; run with the runtimes present:
    ///   $env:EXOQUILL_TESSERACT=...\tesseract.exe
    ///   $env:EXOQUILL_TESSDATA=...\tessdata
    ///   $env:EXOQUILL_TEST_IMAGE=...\test-ocr.png
    ///   cargo test -p exoquill-ai -- --ignored recognizes_layout --nocapture
    #[test]
    #[ignore = "requires a real tesseract + test image via env vars"]
    fn recognizes_layout_from_real_image() {
        let binary = std::env::var("EXOQUILL_TESSERACT")
            .unwrap_or_else(|_| r"C:\Program Files\Tesseract-OCR\tesseract.exe".into());
        let tessdata = std::env::var("EXOQUILL_TESSDATA").ok().map(PathBuf::from);
        let image_path = std::env::var("EXOQUILL_TEST_IMAGE").expect("set EXOQUILL_TEST_IMAGE");
        let image_bytes = std::fs::read(image_path).expect("read test image");

        let ocr = TesseractOcr::new(binary, tessdata);
        let layout = ocr
            .run_layout(
                OcrRequest {
                    image_bytes,
                    languages: "deu+eng".into(),
                },
                &CancelToken::new(),
            )
            .expect("layout ocr failed");
        eprintln!(
            "text: {:?}\nwords: {}, dims: {}x{}",
            layout.text,
            layout.words.len(),
            layout.width,
            layout.height
        );
        assert!(!layout.words.is_empty(), "expected word boxes");
        assert!(layout.width > 0 && layout.height > 0);
        assert!(!layout.text.trim().is_empty());
    }

    #[test]
    fn parse_tsv_skips_empty_and_low_conf_words() {
        let tsv = "level\tpage\tblock\tpar\tline\tword\tleft\ttop\twidth\theight\tconf\ttext\n\
            5\t1\t1\t1\t1\t1\t0\t0\t10\t10\t-1\tghost\n\
            5\t1\t1\t1\t1\t2\t0\t0\t10\t10\t80\t  \n\
            5\t1\t1\t1\t1\t3\t0\t0\t10\t10\t70\treal";
        let layout = TesseractOcr::parse_tsv(tsv);
        assert_eq!(layout.words.len(), 1);
        assert_eq!(layout.text, "real");
    }
}
