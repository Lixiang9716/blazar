#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenceMode {
    Greeting,
    IdleSparkle,
    Listening,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WelcomeState {
    mode: PresenceMode,
    mode_entered_at_ms: u64,
    last_activity_at_ms: u64,
}

impl WelcomeState {
    pub fn new() -> Self {
        Self {
            mode: PresenceMode::Greeting,
            mode_entered_at_ms: 0,
            last_activity_at_ms: 0,
        }
    }

    pub fn mode(&self) -> PresenceMode {
        self.mode
    }

    pub fn tick(self, now_ms: u64, has_input: bool) -> Self {
        let mode_elapsed = now_ms.saturating_sub(self.mode_entered_at_ms);
        let activity_elapsed = now_ms.saturating_sub(self.last_activity_at_ms);

        match (self.mode, has_input) {
            (_, true) if self.mode != PresenceMode::Listening => Self {
                mode: PresenceMode::Listening,
                mode_entered_at_ms: now_ms,
                last_activity_at_ms: now_ms,
            },
            (PresenceMode::Listening, true) => Self {
                mode: PresenceMode::Listening,
                mode_entered_at_ms: self.mode_entered_at_ms,
                last_activity_at_ms: now_ms,
            },
            (PresenceMode::Greeting, false) if mode_elapsed >= 1_200 => Self {
                mode: PresenceMode::IdleSparkle,
                mode_entered_at_ms: now_ms,
                last_activity_at_ms: now_ms,
            },
            (PresenceMode::Listening, false) if activity_elapsed >= 1_500 => Self {
                mode: PresenceMode::IdleSparkle,
                mode_entered_at_ms: now_ms,
                last_activity_at_ms: now_ms,
            },
            _ => self,
        }
    }

    pub fn animation_frame_index(
        self,
        now_ms: u64,
        frame_count: usize,
        frame_interval_ms: u64,
    ) -> usize {
        let elapsed = now_ms.saturating_sub(self.mode_entered_at_ms);
        ((elapsed / frame_interval_ms) as usize) % frame_count
    }
}

impl Default for WelcomeState {
    fn default() -> Self {
        Self::new()
    }
}
