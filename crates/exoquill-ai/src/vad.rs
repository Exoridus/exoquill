//! Voice activity detection provider (product spec §14.3).

use crate::provider::{Provider, ProviderResult};

/// Detects speech in audio frames to drive dictation segmentation. Unlike the
/// other providers VAD is frame-synchronous and cheap, so it has no cancel
/// token — callers simply stop feeding frames.
pub trait VadProvider: Provider {
    /// Speech probability in `[0.0, 1.0]` for one frame of mono PCM samples.
    fn detect(&self, frame: &[f32], sample_rate: u32) -> ProviderResult<f32>;
}
