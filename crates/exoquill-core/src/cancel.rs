//! Cooperative cancellation primitive shared by the job queue and AI providers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A cooperative cancellation handle. Cloning shares the same underlying flag,
/// so a job's worker and the scheduler can observe/signal cancellation from
/// different threads.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    /// Request cancellation. Idempotent.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_is_shared_across_clones() {
        let token = CancelToken::new();
        let clone = token.clone();
        assert!(!clone.is_cancelled());
        token.cancel();
        assert!(clone.is_cancelled());
    }
}
