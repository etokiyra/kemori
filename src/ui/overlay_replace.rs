pub fn render_replace_preview(frame: &mut Frame, area: Rect, preview: &ReplacePreview) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let width = ((area.width as u32 * 4 / 5) as u16)
        .clamp(40, 140)
        .min(area.width.max(1));
    let height = area.height.saturating_sub(2).max(3).min(area.height);
    let rect = centered(area, width, height);

    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::accent())
        .style(Style::default().bg(theme::panel_bg()))
        .title(Line::from(vec![
            Span::styled("✦ ", theme::star()),
            Span::styled(preview.title.clone(), theme::accent_bold()),
            Span::styled(" ✦", theme::star()),
        ]));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    if inner.height == 0 {
        return;
    }

    // Each row needs two lines: the old text and the replacement.
    let visible = (inner.height as usize).saturating_sub(1).max(1) / 2;
    let max_scroll = preview.rows.len().saturating_sub(visible);
    let scroll = preview.scroll.min(max_scroll);
    let mut lines: Vec<Line> = Vec::new();
    for row in preview.rows.iter().skip(scroll).take(visible) {
        lines.push(Line::from(vec![
            Span::styled(format!(" {:>5}  - ", row.line), theme::error_color()),
            Span::styled(row.before.clone(), theme::error_color()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("        + ", theme::success_color()),
            Span::styled(row.after.clone(), theme::success_color()),
        ]));
    }
    let content_area = Rect {
        height: inner.height.saturating_sub(1),
        ..inner
    };
    frame.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().bg(theme::panel_bg())),
        content_area,
    );

    let footer = Line::from(vec![
        Span::styled(" ↑↓ scroll ", theme::muted()),
        Span::styled("·", theme::dim()),
        Span::styled(" Enter/a replace all ", theme::muted()),
        Span::styled("·", theme::dim()),
        Span::styled(" Esc cancel ", theme::muted()),
        Span::styled("·", theme::dim()),
        Span::styled(
            format!(
                " {} match(es) in {} line(s) ",
                preview.matches,
                preview.rows.len()
            ),
            theme::muted(),
        ),
    ]);
    let footer_area = Rect {
        y: inner.y + inner.height.saturating_sub(1),
        height: 1,
        ..inner
    };
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().bg(theme::panel_bg())),
        footer_area,
    );
}
