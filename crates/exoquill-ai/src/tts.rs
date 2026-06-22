//! Text-to-speech provider (product spec §13).

use serde::{Deserialize, Serialize};

use crate::provider::{CancelToken, Provider, ProviderResult};

/// A segment of text to synthesize, plus the tuning knobs the UI exposes. The
/// `Option` fields fall back to each model's defaults when `None`; a provider
/// applies only the ones it understands (XTTS uses `speed` alone).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsRequest {
    pub text: String,
    pub voice_id: String,
    /// Speaking rate; 1.0 = normal, >1 faster. Piper maps this to `length_scale`
    /// (= 1/speed); XTTS passes it through as its `speed`.
    pub speed: f32,
    /// Piper `noise_scale` — expressiveness / timbre variation (default 0.667).
    pub expressiveness: Option<f32>,
    /// Piper `noise_w` — phoneme-duration (cadence) variability (default 0.8).
    pub cadence: Option<f32>,
    /// Seconds of silence after each sentence (Piper, default 0.2).
    pub sentence_silence: Option<f32>,
    /// Zonos `pitch_std` — intonation liveliness; low is monotone, high is lively
    /// (Zonos default 20, our sidecar default 42).
    pub intonation: Option<f32>,
    /// Zonos `fmax` — synthesis frequency ceiling in Hz; lower is warmer/duller,
    /// higher is brighter (default 22050, the native rate's ceiling).
    pub brightness: Option<f32>,
    /// Zonos `emotion` — the 8-value conditioning vector [happiness, sadness,
    /// disgust, fear, surprise, anger, other, neutral]; `None` leaves Zonos' own
    /// default. The UI resolves a mood preset to this vector.
    pub emotion: Option<Vec<f32>>,
}

impl TtsRequest {
    /// A request with model defaults for every knob (just text + voice + speed).
    pub fn new(text: impl Into<String>, voice_id: impl Into<String>, speed: f32) -> Self {
        Self {
            text: text.into(),
            voice_id: voice_id.into(),
            speed,
            expressiveness: None,
            cadence: None,
            sentence_silence: None,
            intonation: None,
            brightness: None,
            emotion: None,
        }
    }
}

/// Synthesized audio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsResponse {
    /// Mono PCM samples normalized to `[-1.0, 1.0]`.
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

/// A selectable voice the provider offers (settings/read-aloud voice picker).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsVoice {
    /// Stable identifier passed back in `TtsRequest::voice_id` (e.g. the Piper
    /// model file stem, `de_DE-thorsten-medium`).
    pub id: String,
    /// Human-readable label for the picker (e.g. `Thorsten — de_DE (medium)`).
    pub display_name: String,
    /// BCP-47-ish language tag (`de_DE`, `en_US`), best-effort from the id.
    pub language: String,
    /// Quality tier (`x_low` | `low` | `medium` | `high`), best-effort.
    pub quality: String,
    /// Synthesis backend that offers this voice (`"piper"` | `"xtts"`). Lets the
    /// UI group voices by backend and route a request to the right provider.
    pub provider: String,
}

/// Pick the optimal synthesis language for `text` — German vs. English, the two
/// languages this app targets. Umlauts/ß are a hard German signal; otherwise a
/// stop-word vote decides, defaulting to German (the app's primary language).
/// Run per segment by the multilingual providers (XTTS, Zonos), so a German note
/// with English quotes reads each part optimally without the user picking.
pub(crate) fn detect_language(text: &str) -> &'static str {
    let lower = text.to_lowercase();
    if lower.chars().any(|c| matches!(c, 'ä' | 'ö' | 'ü' | 'ß')) {
        return "de";
    }
    const DE: &[&str] = &[
        "der", "die", "das", "und", "ist", "nicht", "ein", "eine", "ich", "wir", "mit", "den",
        "dem", "von", "zu", "sich", "auch", "wird", "werden", "oder", "aber", "sind", "im", "des",
        "wie", "noch", "auf", "es", "an", "als",
    ];
    const EN: &[&str] = &[
        "the", "is", "are", "and", "to", "of", "in", "that", "with", "for", "this", "you", "it",
        "on", "be", "as", "at", "or", "an", "we", "not", "but", "by", "from", "can", "will", "has",
        "was", "have", "they",
    ];
    let mut de = 0usize;
    let mut en = 0usize;
    for word in lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        if DE.contains(&word) {
            de += 1;
        }
        if EN.contains(&word) {
            en += 1;
        }
    }
    if en > de {
        "en"
    } else {
        "de"
    }
}

/// Synthesizes speech from text segments (the read-aloud queue calls this per
/// sentence, product spec §13.5).
pub trait TextToSpeechProvider: Provider {
    fn run(&self, request: TtsRequest, cancel: &CancelToken) -> ProviderResult<TtsResponse>;

    /// The voices this provider offers, for the picker. Empty when the provider
    /// has no notion of selectable voices.
    fn voices(&self) -> Vec<TtsVoice> {
        Vec::new()
    }

    /// The id of the voice used when a request names an unknown (or empty) voice.
    fn default_voice(&self) -> Option<String> {
        None
    }
}
