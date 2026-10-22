//! Running, Paused or Stopped (spec §5.1). Hotkeys and card actions ignore it;
//! only automatic triggers ask it.

use serde::{Deserialize, Serialize};

use crate::settings::StartState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Running,
    Paused,
    Stopped,
}

impl RunState {
    pub fn from_start(start: StartState) -> Self {
        match start {
            StartState::Running => Self::Running,
            StartState::Paused => Self::Paused,
            StartState::Stopped => Self::Stopped,
        }
    }

    /// The cycle-state hotkey's order.
    pub fn next(self) -> Self {
        match self {
            Self::Running => Self::Paused,
            Self::Paused => Self::Stopped,
            Self::Stopped => Self::Running,
        }
    }

    /// Ask once per automatic trigger that has work to do. Paused lets exactly
    /// one through unprocessed and goes back to Running.
    pub fn take_automatic(&mut self) -> bool {
        match self {
            Self::Running => true,
            Self::Paused => {
                *self = Self::Running;
                false
            }
            Self::Stopped => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hotkey_cycles_running_paused_stopped() {
        let mut state = RunState::Running;
        let mut seen = Vec::new();
        for _ in 0..4 {
            state = state.next();
            seen.push(state);
        }
        use RunState::*;
        assert_eq!(seen, [Paused, Stopped, Running, Paused]);
    }

    #[test]
    fn paused_skips_exactly_one_trigger_then_runs() {
        let mut state = RunState::Paused;
        assert!(!state.take_automatic());
        assert_eq!(state, RunState::Running);
        assert!(state.take_automatic());
        assert!(state.take_automatic());
    }

    #[test]
    fn stopped_skips_every_trigger() {
        let mut state = RunState::Stopped;
        for _ in 0..3 {
            assert!(!state.take_automatic());
        }
        assert_eq!(state, RunState::Stopped);
    }

    #[test]
    fn the_start_state_comes_from_settings() {
        assert_eq!(RunState::from_start(StartState::Paused), RunState::Paused);
        assert_eq!(RunState::from_start(StartState::Stopped), RunState::Stopped);
        assert_eq!(RunState::from_start(StartState::Running), RunState::Running);
    }
}
