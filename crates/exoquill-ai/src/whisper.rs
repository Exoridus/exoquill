//! Real speech-to-text provider backed by whisper.cpp (sidecar process).
//!
//! Each dictation segment's mono PCM samples are written to a temporary 16 kHz
//! WAV file which `whisper-cli` transcribes; the plain-text result is read from
//! the `.txt` file whisper writes next to the `-of` stem (more robust across
//! whisper.cpp versions than scraping stdout). whisper.cpp is fixed at 16 kHz
//! mono, so callers resample before sending (the frontend capture loop does
//! this) and we just stamp the WAV header accordingly.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::provider::{
    CancelToken, Capability, Health, LicenseInfo, ModelRequirement, Provider, ProviderError,
    ProviderResult,
};
use crate::stt::{SpeechToTextProvider, SttRequest, SttResponse};

/// Disambiguates concurrent temp WAV files within this process.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Speech-to-text via a bundled whisper.cpp executable + ggml model.
pub struct WhisperStt {
    binary: PathBuf,
    model: PathBuf,
    threads: u32,
}

impl WhisperStt {
    pub fn new(binary: impl Into<PathBuf>, model: impl Into<PathBuf>) -> Self {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(4);
        Self {
            binary: binary.into(),
            model: model.into(),
            threads,
        }
    }

    /// Map the note's `language_mode` to a whisper `-l` value. German is the
    /// default (the product is German-first); only explicit `en`/`auto` differ.
    pub(crate) fn language_flag(mode: &str) -> &'static str {
        match mode {
            "en" => "en",
            "auto" => "auto",
            _ => "de",
        }
    }

    /// Build whisper's initial `--prompt` from custom terms so the decoder is
    /// biased toward the user's product/library names. `None` when there are
    /// none, so we don't pass an empty prompt.
    pub(crate) fn build_prompt(custom_terms: &[String]) -> Option<String> {
        let terms: Vec<&str> = custom_terms
            .iter()
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .collect();
        (!terms.is_empty()).then(|| terms.join(", "))
    }

    /// Encode mono `samples` as a 16-bit PCM WAV byte buffer (canonical 44-byte
    /// header). Samples are clamped to `[-1.0, 1.0]` before quantization.
    pub(crate) fn encode_wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
        const BITS_PER_SAMPLE: u16 = 16;
        let block_align: u16 = BITS_PER_SAMPLE / 8; // mono
        let byte_rate = sample_rate * block_align as u32;
        let data_len = samples.len() as u32 * block_align as u32;

        let mut buf = Vec::with_capacity(44 + data_len as usize);
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&(36 + data_len).to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes()); // PCM fmt chunk size
        buf.extend_from_slice(&1u16.to_le_bytes()); // audio format = PCM
        buf.extend_from_slice(&1u16.to_le_bytes()); // channels = mono
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&byte_rate.to_le_bytes());
        buf.extend_from_slice(&block_align.to_le_bytes());
        buf.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_len.to_le_bytes());
        for &sample in samples {
            let scaled = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            buf.extend_from_slice(&scaled.to_le_bytes());
        }
        buf
    }

    /// Join the lines of whisper's `.txt` output into a single normalized line.
    pub(crate) fn parse_transcript(raw: &str) -> String {
        raw.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string()
    }
}

impl Provider for WhisperStt {
    fn id(&self) -> &str {
        "stt.whisper"
    }
    fn display_name(&self) -> &str {
        "Whisper STT"
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
        if !self.model.exists() {
            Health::MissingModel {
                model_id: "stt.whisper".into(),
            }
        } else if self.binary.exists() {
            Health::Ready
        } else {
            Health::Unavailable {
                reason: format!("whisper-cli not found at {:?}", self.binary),
            }
        }
    }
}

impl SpeechToTextProvider for WhisperStt {
    fn run(&self, request: SttRequest, cancel: &CancelToken) -> ProviderResult<SttResponse> {
        if cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        let language = Self::language_flag(&request.language_mode);
        if request.samples.is_empty() {
            return Ok(SttResponse {
                text: String::new(),
                language: Some(language.into()),
                confidence: None,
            });
        }

        // whisper-cli reads from a file and writes the transcript next to the
        // `-of` stem; stage both in temp, unique per process + call, and clean
        // them up once the child exits.
        let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let stem = std::env::temp_dir().join(format!("exoquill-stt-{}-{n}", std::process::id()));
        let wav_path = stem.with_extension("wav");
        let txt_path = stem.with_extension("txt");
        std::fs::write(
            &wav_path,
            Self::encode_wav(&request.samples, request.sample_rate),
        )
        .map_err(|e| ProviderError::Runtime(format!("write temp wav: {e}")))?;

        let mut command = Command::new(&self.binary);
        command
            .arg("-m")
            .arg(&self.model)
            .arg("-f")
            .arg(&wav_path)
            .arg("-l")
            .arg(language)
            .arg("-t")
            .arg(self.threads.to_string())
            .arg("-nt") // no per-segment timestamps
            .arg("-np") // suppress everything but the result
            .arg("-otxt") // write the transcript to <stem>.txt
            .arg("-of")
            .arg(&stem);
        if let Some(prompt) = Self::build_prompt(&request.custom_terms) {
            command.arg("--prompt").arg(prompt);
        }

        let output = match command.output() {
            Ok(output) => output,
            Err(e) => {
                let _ = std::fs::remove_file(&wav_path);
                return Err(ProviderError::Runtime(format!("spawn whisper: {e}")));
            }
        };

        let transcript = std::fs::read_to_string(&txt_path).ok();
        let _ = std::fs::remove_file(&wav_path); // best-effort cleanup
        let _ = std::fs::remove_file(&txt_path);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ProviderError::Runtime(format!(
                "whisper-cli failed: {}",
                stderr.lines().last().unwrap_or("unknown error")
            )));
        }

        let text = Self::parse_transcript(transcript.as_deref().unwrap_or_default());
        Ok(SttResponse {
            text,
            language: Some(language.into()),
            confidence: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_flag_defaults_to_german() {
        assert_eq!(WhisperStt::language_flag("de_en_terms"), "de");
        assert_eq!(WhisperStt::language_flag("anything"), "de");
        assert_eq!(WhisperStt::language_flag("en"), "en");
        assert_eq!(WhisperStt::language_flag("auto"), "auto");
    }

    #[test]
    fn prompt_is_none_without_terms() {
        assert_eq!(WhisperStt::build_prompt(&[]), None);
        assert_eq!(WhisperStt::build_prompt(&["   ".into()]), None);
    }

    #[test]
    fn prompt_joins_nonempty_terms() {
        let terms = vec!["TipTap".into(), "  ".into(), "Tauri".into()];
        assert_eq!(
            WhisperStt::build_prompt(&terms),
            Some("TipTap, Tauri".to_string())
        );
    }

    #[test]
    fn parse_transcript_joins_and_trims_lines() {
        let raw = "\n  Hallo Welt \n\n das ist ein Test \n";
        assert_eq!(
            WhisperStt::parse_transcript(raw),
            "Hallo Welt das ist ein Test"
        );
    }

    #[test]
    fn encode_wav_has_canonical_header() {
        let wav = WhisperStt::encode_wav(&[0.0, 1.0, -1.0], 16_000);
        // 44-byte header + 3 mono 16-bit samples.
        assert_eq!(wav.len(), 44 + 6);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[36..40], b"data");
        // Sample rate at offset 24, channels at offset 22.
        assert_eq!(
            u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]),
            16_000
        );
        assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 1);
        // Full-scale samples clamp to i16::MAX / i16::MIN+1.
        let first = i16::from_le_bytes([wav[44], wav[45]]);
        let second = i16::from_le_bytes([wav[46], wav[47]]);
        let third = i16::from_le_bytes([wav[48], wav[49]]);
        assert_eq!(first, 0);
        assert_eq!(second, i16::MAX);
        assert_eq!(third, -i16::MAX);
    }

    #[test]
    fn empty_samples_yield_empty_transcript() {
        let stt = WhisperStt::new("whisper-cli", "model.bin");
        let response = stt
            .run(
                SttRequest {
                    samples: Vec::new(),
                    sample_rate: 16_000,
                    language_mode: "de_en_terms".into(),
                    custom_terms: Vec::new(),
                },
                &CancelToken::new(),
            )
            .unwrap();
        assert_eq!(response.text, "");
        assert_eq!(response.language.as_deref(), Some("de"));
    }

    #[test]
    fn cancelled_run_returns_cancelled() {
        let stt = WhisperStt::new("whisper-cli", "model.bin");
        let cancel = CancelToken::new();
        cancel.cancel();
        let err = stt
            .run(
                SttRequest {
                    samples: vec![0.1, 0.2],
                    sample_rate: 16_000,
                    language_mode: "de_en_terms".into(),
                    custom_terms: Vec::new(),
                },
                &cancel,
            )
            .unwrap_err();
        assert_eq!(err, ProviderError::Cancelled);
    }

    #[test]
    fn whisper_is_object_safe() {
        let _s: Box<dyn SpeechToTextProvider> = Box::new(WhisperStt::new("whisper-cli", "m.bin"));
    }

    /// Real end-to-end smoke test against the bundled whisper-cli + model.
    /// Ignored by default (needs the runtimes); run it with, e.g.:
    ///   $env:EXOQUILL_WHISPER=...\whisper-cli.exe
    ///   $env:EXOQUILL_WHISPER_MODEL=...\ggml-large-v3-turbo-q5_0.bin
    ///   $env:EXOQUILL_TEST_WAV=...\jfk.wav   # English 16 kHz mono sample
    ///   cargo test -p exoquill-ai -- --ignored transcribes_real_wav --nocapture
    #[test]
    #[ignore = "requires the whisper runtime + a test WAV via env vars"]
    fn transcribes_real_wav() {
        let binary = std::env::var("EXOQUILL_WHISPER").expect("set EXOQUILL_WHISPER");
        let model = std::env::var("EXOQUILL_WHISPER_MODEL").expect("set EXOQUILL_WHISPER_MODEL");
        let wav = std::env::var("EXOQUILL_TEST_WAV").expect("set EXOQUILL_TEST_WAV");
        let (samples, sample_rate) = read_wav_pcm16(&wav);

        let stt = WhisperStt::new(binary, model);
        assert!(
            matches!(stt.health_check(), Health::Ready),
            "whisper runtime is not ready"
        );

        let response = stt
            .run(
                SttRequest {
                    samples,
                    sample_rate,
                    language_mode: "en".into(),
                    custom_terms: Vec::new(),
                },
                &CancelToken::new(),
            )
            .expect("transcription failed");
        eprintln!("transcript: {}", response.text);
        assert!(!response.text.trim().is_empty(), "transcript was empty");
    }

    /// Minimal 16-bit PCM WAV reader for the smoke test: locates the `fmt ` and
    /// `data` chunks and returns normalized mono samples + the sample rate.
    fn read_wav_pcm16(path: &str) -> (Vec<f32>, u32) {
        let bytes = std::fs::read(path).expect("read test wav");
        let find = |tag: &[u8; 4]| {
            bytes
                .windows(4)
                .position(|w| w == tag)
                .unwrap_or_else(|| panic!("missing {:?} chunk", std::str::from_utf8(tag)))
        };
        let fmt = find(b"fmt ");
        let sample_rate = u32::from_le_bytes([
            bytes[fmt + 12],
            bytes[fmt + 13],
            bytes[fmt + 14],
            bytes[fmt + 15],
        ]);
        let data = find(b"data");
        let len = u32::from_le_bytes([
            bytes[data + 4],
            bytes[data + 5],
            bytes[data + 6],
            bytes[data + 7],
        ]) as usize;
        let start = data + 8;
        let samples = bytes[start..start + len]
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
            .collect();
        (samples, sample_rate)
    }
}
