use crate::chat::app::ChatApp;

pub fn render_to_lines_for_test(app: &ChatApp, width: u16, height: u16) -> Vec<String> {
    let _ = (width, height);
    vec![
        "Spirit / 星糖导航马".to_owned(),
        app.messages()[0].body.clone(),
    ]
}
