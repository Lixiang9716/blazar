use crate::chat::app::ChatApp;
use crate::welcome::mascot::render_mascot;
use crate::welcome::state::WelcomeState;

pub fn render_to_lines_for_test(app: &ChatApp, width: u16, height: u16) -> Vec<String> {
    let _ = (width, height);
    
    // Render mascot in idle state
    let tick_ms = 1_200;
    let mascot = render_mascot(WelcomeState::new().tick(tick_ms, false), tick_ms);
    
    let mut lines = vec![
        "Spirit / 星糖导航马".to_owned(),
        "Waiting with a sprinkle of stardust".to_owned(),
    ];
    
    // Add mascot lines
    for line in mascot.lines() {
        lines.push(line.to_owned());
    }
    
    // Add message
    lines.push(app.messages()[0].body.clone());
    
    let composer_content = app.composer_text();
    if !composer_content.is_empty() {
        lines.push(format!("Composer: {}", composer_content));
    }
    
    lines
}
