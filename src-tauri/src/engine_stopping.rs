//! Tracks an engine being stopped outside the `EngineState` lock.
//! `stop_audio_engine` takes the engine out under the lock and stops it
//! after releasing it (finalizing a recording and joining the audio
//! thread), so `start_audio_engine` must not open a second engine on the
//! same devices until that stop has finished.
use std::sync::atomic::{AtomicBool, Ordering};

/// Tauri-managed flag: set while an engine stop is in progress.
#[derive(Default)]
pub struct EngineStopping(AtomicBool);

impl EngineStopping {
    /// Mark a stop as in progress. Cleared when the returned guard drops,
    /// on every path including a panic during the stop. Call it under the
    /// engine lock, and only after actually taking an engine out.
    pub fn begin(&self) -> StoppingGuard<'_> {
        self.0.store(true, Ordering::SeqCst);
        StoppingGuard(&self.0)
    }

    pub fn is_stopping(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Clears the `EngineStopping` flag when dropped.
pub struct StoppingGuard<'a>(&'a AtomicBool);

impl Drop for StoppingGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flag_is_set_for_the_life_of_the_guard() {
        let stopping = EngineStopping::default();
        assert!(!stopping.is_stopping());
        {
            let _guard = stopping.begin();
            assert!(stopping.is_stopping());
        }
        assert!(!stopping.is_stopping());
    }

    #[test]
    fn a_panicking_stop_still_clears_the_flag() {
        let stopping = EngineStopping::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = stopping.begin();
            panic!("stop failed");
        }));
        assert!(result.is_err());
        assert!(!stopping.is_stopping());
    }
}
