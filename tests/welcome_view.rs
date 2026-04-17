use blazar::welcome::mascot::render_mascot;
use blazar::welcome::state::WelcomeState;
use blazar::welcome::view::render_scene;

#[test]
fn welcome_view_contains_brand_copy_and_prompt() {
    let scene = render_scene(WelcomeState::new(), 0);

    assert!(scene.contains("BLAZAR"));
    assert!(scene.contains("Star Sugar Guidepony / 星糖导航马"));
    assert!(scene.contains("A rainbow helper just spotted you"));
    assert!(scene.contains("Describe a task to begin"));
}

#[test]
fn welcome_view_keeps_sprite_and_copy_columns_together() {
    let state = WelcomeState::new();
    let scene = render_scene(state, 0);
    let mascot = render_mascot(state, 0);
    let mascot_width = mascot
        .lines()
        .map(strip_ansi)
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);

    for copy in [
        "BLAZAR",
        "Star Sugar Guidepony / 星糖导航马",
        "A rainbow helper just spotted you",
        "Describe a task to begin",
        "> ",
    ] {
        let raw_line = scene
            .lines()
            .map(strip_ansi)
            .find(|line| line.contains(copy))
            .expect("copy should be present in the composed scene");

        let column = raw_line
            .split_once(copy)
            .map(|(prefix, _)| prefix.chars().count());

        assert_eq!(column, Some(mascot_width + 4));
    }
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
