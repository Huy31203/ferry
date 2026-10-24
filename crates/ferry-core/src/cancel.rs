//! A cheap flag a job's owner flips to stop its encoder or sidecar.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// Whether `other` is this very token (or a clone), whatever its state.
    pub fn same(&self, other: &CancelToken) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_is_identity_not_state() {
        let token = CancelToken::new();
        assert!(token.same(&token.clone()));
        assert!(!token.same(&CancelToken::new()));
    }

    #[test]
    fn clones_share_one_flag() {
        let token = CancelToken::new();
        let seen_by_job = token.clone();
        assert!(!seen_by_job.is_cancelled());
        token.cancel();
        assert!(seen_by_job.is_cancelled());
    }
}
