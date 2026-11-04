// Diff overlay rendering; included from `overlay.rs`.

pub fn render_diff(frame: &mut Frame, area: Rect, diff: &DiffState) {
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
            Span::styled(diff.title.clone(), theme::accent_bold()),
            Span::styled(" ✦", theme::star()),
        ]));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    if inner.height == 0 {
        return;
    }

    // Reserve the last row for a footer hint.
    let visible = (inner.height as usize).saturating_sub(1).max(1);
    let max_scroll = diff.lines.len().saturating_sub(visible);
    let scroll = diff.scroll.min(max_scroll);
    let lines: Vec<Line> = diff
        .lines
        .iter()
        .skip(scroll)
        .take(visible)
        .map(|line| {
            let style = match line.kind {
                DiffLineKind::Add => Style::default().fg(theme::success_color()),
                DiffLineKind::Remove => Style::default().fg(theme::error_color()),
                DiffLineKind::Hunk => Style::default().fg(theme::accent_color()),
                DiffLineKind::Header => Style::default().fg(theme::muted_color()),
                DiffLineKind::Context => Style::default().fg(theme::text_color()),
            };
            Line::from(Span::styled(format!(" {} ", line.text), style))
        })
        .collect();
    let content_area = Rect {
        height: inner.height.saturating_sub(1),
        ..inner
    };
    frame.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().bg(theme::panel_bg())),
        content_area,
    );

    let last = diff.lines.len();
    let footer = Line::from(vec![
        Span::styled(" ↑↓ scroll ", theme::muted()),
        Span::styled("·", theme::dim()),
        Span::styled(format!(" {last} line(s) "), theme::muted()),
        Span::styled("·", theme::dim()),
        Span::styled(" Esc close ", theme::muted()),
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

