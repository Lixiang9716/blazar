use crate::welcome::mascot::{MascotPose, render_pose};
use crate::welcome::state::{PresenceMode, WelcomeState};
use crate::welcome::theme::{MASCOT_ALIAS_ZH, MASCOT_NAME, MASCOT_PALETTE, paint};

pub fn render_scene(state: WelcomeState) -> String {
    let pose = match state.mode() {
        PresenceMode::Greeting => MascotPose::Greeting,
        PresenceMode::IdleSparkle => MascotPose::IdleSparkle,
        PresenceMode::Listening => MascotPose::Listening,
    };

    let right = vec![
        paint("BLAZAR", MASCOT_PALETTE.blue_ansi),
        format!("{MASCOT_NAME} / {MASCOT_ALIAS_ZH}"),
        status_copy(state.mode()).to_string(),
        String::new(),
        "Describe a task to begin".to_string(),
        "> ".to_string(),
    ];

    join_columns(render_pose(pose), &right)
}

fn status_copy(mode: PresenceMode) -> &'static str {
    match mode {
        PresenceMode::Greeting => "A rainbow helper just spotted you",
        PresenceMode::IdleSparkle => "Waiting with a sprinkle of stardust",
        PresenceMode::Listening => "Listening with twinkly focus",
    }
}

fn soft_colorize(line: &str, index: usize) -> String {
    let color = match index % 4 {
        0 => MASCOT_PALETTE.horn_ansi,
        1 => MASCOT_PALETTE.pink_ansi,
        2 => MASCOT_PALETTE.mint_ansi,
        _ => MASCOT_PALETTE.blue_ansi,
    };

    paint(line, color)
}

fn join_columns(left: &[&str], right: &[String]) -> String {
    let left_width = left.iter().map(|line| line.chars().count()).max().unwrap_or(0) + 4;

    let rows = left.len().max(right.len());
    let mut lines = Vec::with_capacity(rows);

    for index in 0..rows {
        let left_line = left.get(index).copied().unwrap_or("");
        let right_line = right.get(index).map(String::as_str).unwrap_or("");
        let padded = format!("{left_line:<width$}", width = left_width);
        lines.push(format!("{}{}", soft_colorize(&padded, index), right_line));
    }

    lines.join("\n")
}
