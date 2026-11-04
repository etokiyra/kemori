// New-project overlay rendering; included from `overlay.rs`.

/// Render the guided "Create a new project" flow.
pub fn render_new_project(frame: &mut Frame, area: Rect, flow: &NewProject) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let width = ((area.width as u32 * 3 / 5) as u16)
        .clamp(42, 86)
        .min(area.width.max(1));
    let on_panel = Style::default().bg(theme::panel_bg());

    match flow.step {
        NewProjectStep::Parent => {
            let rows = directory_rows(&flow.browser.entries);
            let visible = rows.len().min(14);
            let height = (visible as u16 + 4).min(area.height);
            let title = panel_title("New Project  ·  1 of 3  choose a folder");
            let inner = draw_panel(frame, area, title, width, height);
            if inner.height < 4 {
                return;
            }
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Min(1),
                    Constraint::Length(1),
                ])
                .split(inner);
            frame.render_widget(
                Paragraph::new(panel_subtitle(
                    &compact_path(&flow.browser.current),
                    chunks[0].width,
                ))
                .style(on_panel),
                chunks[0],
            );
            render_divider(frame, chunks[1]);
            render_panel_rows(frame, chunks[2], &rows, flow.browser.selected);
            match &flow.error {
                Some(error) => render_footer(frame, chunks[3], error, true),
                None => render_footer(
                    frame,
                    chunks[3],
                    "↑↓ move  ·  Enter choose / open  ·  ← up  ·  Esc cancel",
                    false,
                ),
            }
        }
        NewProjectStep::Name => {
            let height = 7.min(area.height);
            let title = panel_title("New Project  ·  2 of 3  name it");
            let inner = draw_panel(frame, area, title, width, height);
            if inner.height == 0 {
                return;
            }
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Min(0),
                ])
                .split(inner);
            frame.render_widget(
                Paragraph::new(panel_subtitle(
                    &format!("in {}", compact_path(&flow.parent)),
                    chunks[0].width,
                ))
                .style(on_panel),
                chunks[0],
            );
            render_divider(frame, chunks[1]);
            let input = if flow.name.is_empty() {
                Span::styled("project-name", theme::muted())
            } else {
                Span::styled(flow.name.clone(), theme::bright())
            };
            frame.render_widget(
                Paragraph::new(Line::from(vec![Span::styled(" ❯ ", theme::star()), input]))
                    .style(on_panel),
                chunks[2],
            );
            let target = if flow.name.trim().is_empty() {
                "will be created here".to_string()
            } else {
                format!(
                    "creates {}",
                    compact_path(&flow.parent.join(flow.name.trim()))
                )
            };
            frame.render_widget(
                Paragraph::new(panel_subtitle(&target, chunks[3].width)).style(on_panel),
                chunks[3],
            );
            if chunks[4].height > 0 {
                match &flow.error {
                    Some(error) => render_footer(frame, chunks[4], error, true),
                    None => render_footer(frame, chunks[4], "Enter continue  ·  Esc back", false),
                }
            }
            // Place the terminal cursor in the input row.
            let cursor_x = chunks[2].x + 3 + flow.name.chars().count() as u16;
            if cursor_x < chunks[2].x + chunks[2].width {
                frame.set_cursor_position((cursor_x, chunks[2].y));
            }
        }
        NewProjectStep::Language => {
            let rows: Vec<(String, String)> = create::CREATABLE
                .iter()
                .map(|language| {
                    (
                        language.name().to_string(),
                        create::describe(*language).to_string(),
                    )
                })
                .collect();
            let visible = rows.len();
            let height = (visible as u16 + 5).min(area.height);
            let title = panel_title("New Project  ·  3 of 3  language");
            let inner = draw_panel(frame, area, title, width, height);
            if inner.height < 4 {
                return;
            }
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Min(1),
                    Constraint::Length(1),
                ])
                .split(inner);
            frame.render_widget(
                Paragraph::new(panel_subtitle(
                    &format!("{} in {}", flow.name, compact_path(&flow.parent)),
                    chunks[0].width,
                ))
                .style(on_panel),
                chunks[0],
            );
            render_divider(frame, chunks[1]);
            render_panel_rows(frame, chunks[2], &rows, flow.language);
            match &flow.error {
                Some(error) => render_footer(frame, chunks[3], error, true),
                None => render_footer(
                    frame,
                    chunks[3],
                    "↑↓ choose  ·  Enter create  ·  Esc back",
                    false,
                ),
            }
        }
    }
}
