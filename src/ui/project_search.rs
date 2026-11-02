//! The project search and replace overlay.
//!
//! A centred form rather than a panel: search is a deliberate, short-lived
//! action, and the editor underneath should not move while it is open. The
//! results are read-only until `Enter` opens one; the replacement preview uses
//! the same matcher the worker will apply.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};

use crate::app::project_search::{ProjectSearchField, ProjectSearchState};
use crate::ui::{art, centered, theme};

/// Render the search overlay.
pub fn render(frame: &mut Frame, area: Rect, state: &ProjectSearchState) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let width = ((area.width as u32 * 9 / 10) as u16)
        .clamp(50, 140)
        .min(area.width.max(1));
    let height = ((area.height as u32 * 4 / 5) as u16)
        .clamp(10, 40)
        .min(area.height.max(1));
    let rect = centered(area, width, height);

    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::accent())
        .style(Style::default().bg(theme::panel_bg()))
        .title(Line::from(vec![
            Span::styled("✦ ", theme::star()),
            Span::styled("Search in Project", theme::accent_bold()),
            Span::styled(" ✦", theme::star()),
        ]));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    // Four form rows, a divider, the results and a footer.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Find / Replace / Include / Exclude
            Constraint::Length(1), // divider
            Constraint::Min(1),    // results
            Constraint::Length(1), // status and keys
        ])
        .split(inner);

    render_fields(frame, chunks[0], state);
    frame.render_widget(
        Paragraph::new(art::divider(inner.width as usize, Some("results"))),
        chunks[1],
    );
    render_results(frame, chunks[2], state);
    render_footer(frame, chunks[3], state);
}

fn render_fields(frame: &mut Frame, area: Rect, state: &ProjectSearchState) {
    let rows = [
        (ProjectSearchField::Query, &state.query, "text to find", ""),
        (
            ProjectSearchField::Replacement,
            &state.replacement,
            "(none)",
            "",
        ),
        (
            ProjectSearchField::Include,
            &state.options.include,
            "e.g. src/**, *.rs",
            "",
        ),
        (
            ProjectSearchField::Exclude,
            &state.options.exclude,
            "e.g. target/**",
            "",
        ),
    ];
    let mut lines = Vec::new();
    for (field, value, placeholder, _) in rows.iter() {
        let active = state.field == *field;
        let mut spans = vec![
            Span::styled(if active { "▏" } else { " " }, theme::accent()),
            Span::styled(format!(" {:<9}", field.label()), theme::muted()),
        ];
        let style = if active {
            theme::pill(theme::menu_selected_bg(), theme::text_bright())
        } else {
            theme::text()
        };
        if value.is_empty() {
            spans.push(Span::styled(format!(" {placeholder} "), theme::dim()));
        } else {
            spans.push(Span::styled(format!(" {value} "), style));
        }
        if *field == ProjectSearchField::Query {
            spans.push(Span::styled(" ", theme::dim()));
            spans.push(option_chip("Aa", state.options.case_sensitive));
            spans.push(Span::styled(" ", theme::dim()));
            spans.push(option_chip("W", state.options.whole_word));
            spans.push(Span::styled(" ", theme::dim()));
            spans.push(option_chip(".*", state.options.regex));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(Text::from(lines)), area);

    // Put the terminal cursor at the end of the active field, matching the
    // character-based positions used everywhere else.
    let (row, value) = match state.field {
        ProjectSearchField::Query => (0, &state.query),
        ProjectSearchField::Replacement => (1, &state.replacement),
        ProjectSearchField::Include => (2, &state.options.include),
        ProjectSearchField::Exclude => (3, &state.options.exclude),
        ProjectSearchField::Results => return,
    };
    let width = 1 + 10 + 1 + value.chars().count();
    if width < area.width as usize {
        frame.set_cursor_position((area.x + width as u16, area.y + row));
    }
}

fn option_chip(label: &str, on: bool) -> Span<'static> {
    let text = format!(" {label} ");
    if on {
        Span::styled(text, theme::pill(theme::accent_soft(), theme::panel_bg()))
    } else {
        Span::styled(text, theme::muted())
    }
}

fn render_results(frame: &mut Frame, area: Rect, state: &ProjectSearchState) {
    if area.height == 0 {
        return;
    }
    if state.results.is_empty() {
        let hint = if state.running {
            "Searching…"
        } else {
            "Type a query, then Enter searches the project."
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(format!(" {hint}"), theme::muted()))),
            area,
        );
        return;
    }

    enum Row {
        File(String, usize),
        Match(usize),
    }
    let mut rows = Vec::new();
    let mut last: Option<&std::path::Path> = None;
    for (index, entry) in state.results.iter().enumerate() {
        if last != Some(entry.path.as_path()) {
            let name = entry
                .path
                .strip_prefix(&state.root)
                .unwrap_or(&entry.path)
                .display()
                .to_string();
            let count = state
                .results
                .iter()
                .filter(|other| other.path == entry.path)
                .count();
            rows.push(Row::File(name, count));
            last = Some(entry.path.as_path());
        }
        rows.push(Row::Match(index));
    }
    let selected_row = rows
        .iter()
        .position(|row| matches!(row, Row::Match(index) if *index == state.selected))
        .unwrap_or(0);
    let visible = area.height as usize;
    let scroll = scroll_for(selected_row, rows.len(), visible);

    let lines: Vec<Line> = rows
        .iter()
        .skip(scroll)
        .take(visible)
        .map(|row| match row {
            Row::File(name, count) => Line::from(vec![
                Span::styled(format!("{} ", art::POINTER), theme::accent()),
                Span::styled((*name).to_string(), theme::accent_bold()),
                Span::styled(format!(" ({count})"), theme::dim()),
            ]),
            Row::Match(index) => {
                let entry = &state.results[*index];
                let selected = *index == state.selected;
                let style = if selected {
                    theme::selectable(theme::menu_selected_bg())
                } else {
                    theme::text()
                };
                let mut spans = vec![Span::styled(
                    format!(" {:>5}:{:<3} {}", entry.line + 1, entry.col + 1, entry.text),
                    style,
                )];
                if let Some(preview) = &entry.preview
                    && preview != &entry.text
                {
                    spans.push(Span::styled(
                        format!("  -> {preview}"),
                        if selected {
                            theme::success().bg(theme::menu_selected_bg())
                        } else {
                            theme::success()
                        },
                    ));
                }
                Line::from(spans)
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

fn render_footer(frame: &mut Frame, area: Rect, state: &ProjectSearchState) {
    if area.height == 0 {
        return;
    }
    let mut spans = Vec::new();
    if let Some(error) = &state.error {
        spans.push(Span::styled(format!(" {error} "), theme::error()));
    } else if let Some(status) = &state.status {
        spans.push(Span::styled(format!(" {status} "), theme::muted()));
    } else {
        spans.push(Span::styled(" Ready ", theme::muted()));
    }
    spans.push(Span::styled("·", theme::dim()));
    let keys = if state.field == ProjectSearchField::Results {
        " Enter open · Alt+Enter replace · Tab fields · Esc close "
    } else {
        " Enter search · Tab fields · Alt+Enter replace · Esc close "
    };
    spans.push(Span::styled(keys, theme::muted()));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Keep `selected` inside a window of `visible` rows, clamped to the content.
fn scroll_for(selected: usize, total: usize, visible: usize) -> usize {
    if visible == 0 {
        return 0;
    }
    let max_scroll = total.saturating_sub(visible);
    let mut scroll = selected.min(max_scroll);
    if selected >= scroll + visible {
        scroll = (selected + 1 - visible).min(max_scroll);
    }
    scroll
}
