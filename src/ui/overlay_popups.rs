// Completion and hover popups; included from `overlay.rs`.

pub fn render_completion(
    frame: &mut Frame,
    area: Rect,
    state: &CompletionState,
    anchor: Option<(u16, u16)>,
) {
    if state.items.is_empty() || area.width == 0 || area.height == 0 {
        return;
    }
    const MAX_ROWS: usize = 8;

    let label_width = state
        .items
        .iter()
        .map(|item| item.label.chars().count())
        .max()
        .unwrap_or(0);
    let width = ((label_width + 7).clamp(14, 48) as u16).min(area.width);
    let rows = state.items.len().min(MAX_ROWS);
    let height = (rows as u16 + 2).min(area.height);

    let (anchor_x, anchor_y) = anchor.unwrap_or((area.x, area.y));
    let x = anchor_x.min(area.x + area.width.saturating_sub(width));
    let below = anchor_y.saturating_add(1);
    let y = if below + height <= area.y + area.height {
        below
    } else {
        anchor_y.saturating_sub(height).max(area.y)
    };
    let rect = Rect {
        x,
        y,
        width,
        height,
    };

    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::accent())
        .style(Style::default().bg(theme::panel_bg()))
        .title(Line::from(vec![
            Span::styled("✦ ", theme::star()),
            Span::styled("complete", theme::accent_bold()),
        ]));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    if inner.height == 0 {
        return;
    }

    let visible = inner.height as usize;
    let start = if state.selected >= visible {
        state.selected + 1 - visible
    } else {
        0
    };
    let mut lines: Vec<Line> = Vec::new();
    for index in start..(start + visible).min(state.items.len()) {
        let item = &state.items[index];
        let selected = index == state.selected;
        let label_style = if selected {
            theme::bright_bold()
        } else {
            theme::text()
        };
        let kind_style = if selected {
            theme::accent()
        } else {
            theme::dim()
        };
        let marker = if selected {
            Span::styled(" ❯ ", theme::star())
        } else {
            Span::raw("   ")
        };
        let mut line = Line::from(vec![
            marker,
            Span::styled(format!("{} ", item.kind.glyph()), kind_style),
            Span::styled(item.label.clone(), label_style),
        ])
        .style(Style::default().bg(theme::panel_bg()));
        if selected {
            line = line.style(theme::selectable(theme::menu_selected_bg()));
        }
        lines.push(line);
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().bg(theme::panel_bg())),
        inner,
    );
}

/// Render the hover popup, anchored just below the cursor.
pub fn render_hover(frame: &mut Frame, area: Rect, hover: &HoverState, anchor: Option<(u16, u16)>) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let body_width = hover
        .body
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    let width =
        ((hover.title.chars().count().max(body_width) + 4).clamp(18, 64) as u16).min(area.width);
    let rows = hover.body.len().max(1);
    let height = ((rows + 2) as u16).min(area.height);

    let (anchor_x, anchor_y) = anchor.unwrap_or((area.x, area.y));
    let x = anchor_x.min(area.x + area.width.saturating_sub(width));
    let below = anchor_y.saturating_add(1);
    let y = if below + height <= area.y + area.height {
        below
    } else {
        anchor_y.saturating_sub(height).max(area.y)
    };
    let rect = Rect {
        x,
        y,
        width,
        height,
    };

    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::accent())
        .style(Style::default().bg(theme::panel_bg()))
        .title(Line::from(vec![
            Span::styled("✦ ", theme::star()),
            Span::styled(hover.title.clone(), theme::accent_bold()),
        ]));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    if inner.height == 0 {
        return;
    }

    let lines: Vec<Line> = hover
        .body
        .iter()
        .take(inner.height as usize)
        .map(|line| Line::from(Span::styled(line.clone(), theme::text())))
        .collect();
    frame.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().bg(theme::panel_bg())),
        inner,
    );
}

