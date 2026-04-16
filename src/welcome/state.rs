#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenceMode {
    OnWatch,
    TurningToUser,
    IdleMonitor,
    TypingFocus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WelcomeState {
    mode: PresenceMode,
    entered_at_ms: u64,
}

impl WelcomeState {
    pub fn new() -> Self {
        Self {
            mode: PresenceMode::OnWatch,
            entered_at_ms: 0,
        }
    }

    pub fn mode(&self) -> PresenceMode {
        self.mode
    }

    pub fn tick(self, now_ms: u64, has_input: bool) -> Self {
        let elapsed = now_ms.saturating_sub(self.entered_at_ms);

        match (self.mode, has_input, elapsed) {
            (_, true, _) => Self {
                mode: PresenceMode::TypingFocus,
                entered_at_ms: now_ms,
            },
            (PresenceMode::OnWatch, false, elapsed_ms) if elapsed_ms >= 500 => Self {
                mode: PresenceMode::TurningToUser,
                entered_at_ms: now_ms,
            },
            (PresenceMode::TurningToUser, false, elapsed_ms) if elapsed_ms >= 700 => Self {
                mode: PresenceMode::IdleMonitor,
                entered_at_ms: now_ms,
            },
            (PresenceMode::TypingFocus, false, elapsed_ms) if elapsed_ms >= 1_000 => Self {
                mode: PresenceMode::IdleMonitor,
                entered_at_ms: now_ms,
            },
            (PresenceMode::IdleMonitor, false, elapsed_ms) if elapsed_ms >= 5_000 => Self {
                mode: PresenceMode::OnWatch,
                entered_at_ms: now_ms,
            },
            _ => self,
        }
    }
}

impl Default for WelcomeState {
    fn default() -> Self {
        Self::new()
    }
}
