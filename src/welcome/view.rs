use crate::welcome::mascot::render_mascot;
use crate::welcome::state::{PresenceMode, WelcomeState};
use crate::welcome::theme::{MASCOT_ALIAS_ZH, MASCOT_NAME, MASCOT_PALETTE, paint};

pub fn render_scene(state: WelcomeState, now_ms: u64) -> String {
    let left = render_mascot(state, now_ms)
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let left_raw_width = left
        .iter()
        .map(|line| strip_ansi(line).chars().count())
        .max()
        .unwrap_or(0);

    let right = vec![
        paint("BLAZAR", MASCOT_PALETTE.blue_ansi),
        format!("{MASCOT_NAME} / {MASCOT_ALIAS_ZH}"),
        status_copy(state.mode()).to_string(),
        String::new(),
        "Describe a task to begin".to_string(),
        "> ".to_string(),
    ];

    join_columns(&left, left_raw_width + 4, &right)
}

fn status_copy(mode: PresenceMode) -> &'static str {
    match mode {
        PresenceMode::Greeting => "A rainbow helper just spotted you",
        PresenceMode::IdleSparkle => "Waiting with a sprinkle of stardust",
        PresenceMode::Listening => "Listening with twinkly focus",
    }
}

fn join_columns(left: &[String], left_width: usize, right: &[String]) -> String {
    let rows = left.len().max(right.len());
    let mut lines = Vec::with_capacity(rows);

    for index in 0..rows {
        let left_line = left.get(index).map(String::as_str).unwrap_or("");
        let right_line = right.get(index).map(String::as_str).unwrap_or("");
        let left_visible_width = strip_ansi(left_line).chars().count();
        let padding = " ".repeat(left_width.saturating_sub(left_visible_width));
        lines.push(format!("{left_line}{padding}{right_line}"));
    }

    lines.join("\n")
}

fn strip_ansi(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.next_if_eq(&'[').is_some() {
            for next in chars.by_ref() {
                if next == 'm' {
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }

    out
}
