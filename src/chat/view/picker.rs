use crate::chat::app::ChatApp;
use crate::chat::picker::PICKER_PAGE_SIZE;
use crate::chat::theme::{BASE03, ChatTheme};
use core::cmp;
use ratatui_core::{
    layout::{Constraint, Rect},
    terminal::Frame,
    text::{Line, Span},
};
use ratatui_macros::vertical;
use ratatui_widgets::block::Block;
use ratatui_widgets::borders::BorderType;
use ratatui_widgets::paragraph::Paragraph;
use tui_overlay::{Anchor, Backdrop, Overlay, Slide};
use tui_widget_list::{ListBuilder, ListView};

pub(super) fn render_picker(
    frame: &mut Frame,
    full_area: Rect,
    app: &mut ChatApp,
    theme: &ChatTheme,
) {
    let filtered_items: Vec<(String, String)> = app
        .picker
        .filtered_items()
        .into_iter()
        .map(|item| (item.label.clone(), item.description.clone()))
        .collect();
    let total = filtered_items.len();
    if full_area.width < 8 || full_area.height < 6 {
        return;
    }

    let visible_count = total.min(PICKER_PAGE_SIZE) as u16;
    // title(1) + visible items + footer(1) + border(2)
    let picker_h = cmp::min(visible_count + 4, full_area.height.saturating_sub(2));
    let picker_w = cmp::min(50, full_area.width.saturating_sub(4));

    let overlay = Overlay::new()
        .anchor(Anchor::BottomLeft)
        .offset(2, -2)
        .width(Constraint::Length(picker_w))
        .height(Constraint::Length(picker_h))
        .slide(Slide::Bottom)
        .backdrop(Backdrop::new(BASE03))
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(theme.picker_title)
                .title(Line::from(Span::styled(
                    format!(" {} ", app.picker.title),
                    theme.picker_title,
                ))),
        );
    frame.render_stateful_widget(overlay, full_area, app.picker.overlay_state_mut());

    let Some(inner) = app.picker.overlay_state().inner_area() else {
        return;
    };
    if inner.height < 2 || inner.width < 4 || picker_h == 0 || picker_w == 0 {
        return;
    }

    // Reserve last row for footer
    let items_h = inner.height.saturating_sub(1);
    let [items_area, footer_area] = vertical![>=(items_h), ==1].areas(inner);

    let has_up = app.picker.has_scroll_up();
    let has_down = app.picker.has_scroll_down();
    let top_hint = u16::from(has_up);
    let bottom_hint = u16::from(has_down);
    if items_area.height <= top_hint + bottom_hint {
        return;
    }
    let [top_hint_area, list_area, bottom_hint_area] =
        vertical![==(top_hint), >=1, ==(bottom_hint)].areas(items_area);

    if has_up {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled("  ▲ more", theme.dim_text))),
            top_hint_area,
        );
    }

    if total == 0 {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  No matching commands",
                theme.dim_text,
            ))),
            list_area,
        );
    } else {
        let list_builder = ListBuilder::new(|context| {
            let item = &filtered_items[context.index];
            let marker = if context.is_selected { "› " } else { "  " };
            let label_style = if context.is_selected {
                theme.picker_selected
            } else {
                theme.picker_item
            };

            let mut spans = vec![
                Span::styled(marker, label_style),
                Span::styled(item.0.clone(), label_style),
            ];
            if !item.1.is_empty() {
                spans.push(Span::styled(format!("  {}", item.1), theme.picker_desc));
            }

            (Line::from(spans), 1)
        });
        let list = ListView::new(list_builder, total).scroll_padding(1);
        frame.render_stateful_widget(list, list_area, app.picker.list_state_mut());
    }

    if has_down {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled("  ▼ more", theme.dim_text))),
            bottom_hint_area,
        );
    }

    // Footer with count info
    let footer_text = format!(
        "↑↓ navigate · enter select · esc cancel  ({}/{})",
        app.picker.selected_index().map_or(0, |index| index + 1),
        total
    );
    let footer = Line::from(Span::styled(footer_text, theme.dim_text));
    frame.render_widget(Paragraph::new(footer), footer_area);
}
