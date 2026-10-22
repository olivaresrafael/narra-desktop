//! Cooperative cancellation shared by every long-running core operation
//! (Git, agent tasks, future model calls).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Cooperative cancellation for a running operation.
///
/// Cheap to clone (shared atomic flag). The owner calls [`cancel`](Self::cancel)
/// from any thread; the worker observes [`is_cancelled`](Self::is_cancelled)
/// between stages and kills its child work, failing with a `Cancelled` error.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    flag: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    /// Request cancellation. Idempotent and safe to call from any thread.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn cancel_is_visible_across_clones_and_threads() {
        let token = CancellationToken::new();
        let clone = token.clone();
        let handle = thread::spawn(move || clone.cancel());
        handle.join().expect("cancel thread");
        assert!(token.is_cancelled());
    }

    #[test]
    fn fresh_token_is_not_cancelled() {
        assert!(!CancellationToken::new().is_cancelled());
    }
}
