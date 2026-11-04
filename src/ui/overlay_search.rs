// Prompt and find/search bar rendering; included from `overlay.rs`.

/// Render a single-line text prompt.
pub fn render_prompt(frame: &mut Frame, area: Rect, prompt: &Prompt) {
    let width = (area.width.saturating_sub(8)).clamp(28, 72);
    // A little extra height buys a hint row; small terminals keep it minimal.
    let height = if area.height >= 6 { 4 } else { 3 };
    let rect = centered(area, width, height);
    let on_panel = Style::default().bg(theme::panel_bg());

    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::accent())
        .style(on_panel)
        .title(Line::from(vec![
            Span::styled("✦ ", theme::star()),
            Span::styled(prompt.label.clone(), theme::accent_bold()),
            Span::styled(" ✦", theme::star()),
        ]));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    if inner.height == 0 {
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    let input = if prompt.input.is_empty() {
        Span::styled(prompt.placeholder.clone(), theme::muted())
    } else {
        Span::styled(prompt.input.clone(), theme::bright())
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(" ❯ ", theme::star()), input])).style(on_panel),
        rows[0],
    );

    if rows.len() > 1 && rows[1].height > 0 {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "   Enter confirm  ·  Esc cancel",
                theme::dim(),
            ))
            .style(on_panel),
            rows[1],
        );
    }

    let cursor_x = rows[0].x + 3 + prompt.input.chars().count() as u16;
    if cursor_x < rows[0].x + rows[0].width {
        frame.set_cursor_position((cursor_x, rows[0].y));
    }
}

/// Render the find / replace bar as a solid strip above the statusline.
pub fn render_search(frame: &mut Frame, area: Rect, search: &Search) {
    let on_panel = Style::default().bg(theme::panel_bg());
    let query_focused = search.field == SearchField::Query;
    let mut lines: Vec<Line> = Vec::new();

    let query_pill = if query_focused {
        theme::pill(theme::accent_soft(), theme::panel_bg())
    } else {
        theme::pill(theme::muted_color(), theme::panel_bg())
    };

    let counter = if search.query.is_empty() {
        String::new()
    } else if search.matches.is_empty() {
        "  no matches".to_string()
    } else {
        let current = search.current.map(|c| c + 1).unwrap_or(0);
        format!("  {current}/{}", search.matches.len())
    };

    let mut query_spans = vec![
        Span::styled(" find ", query_pill),
        Span::styled(" ", on_panel),
        Span::styled(search.query.clone(), theme::bright()),
    ];
    if !counter.is_empty() {
        query_spans.push(Span::styled(counter, theme::muted()));
    }
    if search.case_sensitive {
        query_spans.push(Span::styled("  Aa", theme::accent_bold()));
    }
    if search.whole_word {
        query_spans.push(Span::styled("  |ab|", theme::accent_bold()));
    }
    if search.regex {
        query_spans.push(Span::styled("  .*", theme::accent_bold()));
    }
    if search.scope.is_some() {
        query_spans.push(Span::styled("  [selection]", theme::accent_bold()));
    }
    if let Some(error) = &search.regex_error {
        query_spans.push(Span::styled(format!("  {error}"), theme::error()));
    }
    pad_line(&mut query_spans, area.width);
    lines.push(Line::from(query_spans));

    if search.replace_mode {
        let replace_pill = if query_focused {
            theme::pill(theme::muted_color(), theme::panel_bg())
        } else {
            theme::pill(theme::accent_soft(), theme::panel_bg())
        };
        let mut spans = vec![
            Span::styled(" replace ", replace_pill),
            Span::styled(" ", on_panel),
            Span::styled(search.replacement.clone(), theme::bright()),
        ];
        if !search.replacement.is_empty() {
            spans.push(Span::styled("  Alt+P preview", theme::muted()));
        }
        pad_line(&mut spans, area.width);
        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(Text::from(lines)), area);

    let cursor_col = if query_focused {
        search.query.chars().count()
    } else {
        search.replacement.chars().count()
    };
    let cursor_y = area.y + if query_focused { 0 } else { 1 };
    let cursor_x = area.x + 7 + cursor_col as u16;
    if cursor_x < area.x + area.width {
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}
