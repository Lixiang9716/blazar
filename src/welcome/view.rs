use crate::welcome::mascot::{MascotPose, render_pose};
use crate::welcome::state::{PresenceMode, WelcomeState};
use crate::welcome::theme::{MASCOT_ALIAS_ZH, MASCOT_NAME};

pub fn render_scene(state: WelcomeState) -> String {
    let pose = match state.mode() {
        PresenceMode::OnWatch => MascotPose::OnWatch,
        PresenceMode::TurningToUser => MascotPose::TurningToUser,
        PresenceMode::IdleMonitor => MascotPose::IdleMonitor,
        PresenceMode::TypingFocus => MascotPose::TypingFocus,
    };

    let sprite = render_pose(pose);
    let right = vec![
        "BLAZAR".to_string(),
        format!("{MASCOT_NAME} / {MASCOT_ALIAS_ZH}"),
        status_copy(state.mode()).to_string(),
        String::new(),
        "Describe a task to begin".to_string(),
        "> ".to_string(),
    ];

    join_columns(sprite, &right)
}

fn status_copy(mode: PresenceMode) -> &'static str {
    match mode {
        PresenceMode::OnWatch => "Calibrating star map",
        PresenceMode::TurningToUser => "Turning toward your terminal",
        PresenceMode::IdleMonitor => "Standing by for your request",
        PresenceMode::TypingFocus => "Listening for your request",
    }
}

fn join_columns(left: &[&str], right: &[String]) -> String {
    let left_width = left
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0)
        + 4;

    let rows = left.len().max(right.len());
    let mut lines = Vec::with_capacity(rows);

    for index in 0..rows {
        let left_line = left.get(index).copied().unwrap_or("");
        let right_line = right.get(index).map(String::as_str).unwrap_or("");
        lines.push(format!(
            "{left_line:<width$}{right_line}",
            width = left_width
        ));
    }

    lines.join("\n")
}
