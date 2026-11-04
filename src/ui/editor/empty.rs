//! Empty-editor placeholder rendering.

use super::*;

pub(super) fn render_empty(frame: &mut Frame, area: Rect, focused: bool) {
    let mut lines: Vec<Line> = vec![Line::from("")];
    lines.extend(art::art_lines(art::CAT, theme::dim()));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("an empty file · start typing", theme::muted())).centered());

    let block_height = lines.len() as u16;
    if area.height > block_height {
        let top = (area.height - block_height) / 2;
        let mut padded: Vec<Line> = (0..top).map(|_| Line::from("")).collect();
        padded.extend(lines);
        lines = padded;
    }
    frame.render_widget(Paragraph::new(Text::from(lines)), area);

    if focused {
        frame.set_cursor_position((area.x, area.y));
    }
}
