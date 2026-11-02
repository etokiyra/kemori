//! The bottom panel: Problems, Output and Debug Console.
//!
//! A bordered strip below the editor, in the same quiet vocabulary as the rest
//! of Kemori. It uses the terminal's own background (no opaque surface), and
//! the border carries the tab strip and the contextual keys so the content
//! itself stays uncluttered.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Paragraph};

use crate::app::panel::{PanelTab, ProblemEntry};
use crate::app::{App, Focus};
use crate::language::diagnostics::Severity;
use crate::ui::theme;

/// Draw the panel into `area`.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::border(app.focus == Focus::Panel))
        .title(tab_strip(app))
        .title_bottom(keys_line(app));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    match app.panel.tab {
        PanelTab::Problems => render_problems(frame, inner, app),
        PanelTab::Output => render_output(frame, inner, app),
        PanelTab::DebugConsole => render_debug_console(frame, inner, app),
    }
}

/// The tab strip in the top border: active tab filled, the rest quiet.
fn tab_strip(app: &App) -> Line<'static> {
    let problem_count = app.panel_problems().len();
    let mut spans = vec![Span::raw(" ")];
    for (index, tab) in PanelTab::ALL.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" ", theme::dim()));
        }
        let label = match tab {
            PanelTab::Problems => format!("{} ({problem_count})", tab.label()),
            _ => tab.label().to_string(),
        };
        if *tab == app.panel.tab {
            spans.push(Span::styled(
                format!(" {label} "),
                theme::pill(theme::menu_selected_bg(), theme::text_bright()),
            ));
        } else {
            spans.push(Span::styled(format!(" {label} "), theme::muted()));
        }
    }
    spans.push(Span::raw(" "));
    Line::from(spans)
}

/// Contextual keys in the bottom border.
fn keys_line(app: &App) -> Line<'static> {
    let mut spans = vec![
        Span::styled(" Esc editor ", theme::muted()),
        Span::styled("·", theme::dim()),
        Span::styled(" Tab switch ", theme::muted()),
    ];
    if matches!(app.panel.tab, PanelTab::Problems | PanelTab::Output) {
        spans.push(Span::styled("·", theme::dim()));
        spans.push(Span::styled(" Enter open ", theme::muted()));
    }
    if app.panel.tab == PanelTab::Output {
        spans.push(Span::styled("·", theme::dim()));
        spans.push(Span::styled(" r rerun ", theme::muted()));
        spans.push(Span::styled("·", theme::dim()));
        spans.push(Span::styled(" x cancel ", theme::muted()));
    }
    spans.push(Span::styled("·", theme::dim()));
    spans.push(Span::styled(" c clear ", theme::muted()));
    spans.push(Span::styled("·", theme::dim()));
    spans.push(Span::styled(" Alt+↑↓ resize ", theme::muted()));
    Line::from(spans)
}

fn render_problems(frame: &mut Frame, area: Rect, app: &App) {
    let problems = app.panel_problems();
    if problems.is_empty() {
        render_message(
            frame,
            area,
            "No problems in the open files.",
            "F8 jumps to the next problem; the language server adds more as it connects.",
        );
        return;
    }

    // Rows alternate file headers and problem lines. Selection counts only
    // problems, so moving through the list never lands on a header.
    enum Row {
        File(String),
        Problem(usize),
    }
    let mut rows = Vec::new();
    let mut last_path: Option<&std::path::Path> = None;
    for (index, problem) in problems.iter().enumerate() {
        if last_path != Some(problem.path.as_path()) {
            rows.push(Row::File(problem.file.clone()));
            last_path = Some(problem.path.as_path());
        }
        rows.push(Row::Problem(index));
    }

    let selected_row = rows
        .iter()
        .position(|row| matches!(row, Row::Problem(index) if *index == app.panel.problem_selected))
        .unwrap_or(0);
    let visible = area.height as usize;
    let scroll = scroll_for(selected_row, rows.len(), visible);

    let lines: Vec<Line> = rows
        .iter()
        .skip(scroll)
        .take(visible)
        .map(|row| match row {
            Row::File(file) => Line::from(vec![
                Span::styled(format!("{} ", crate::ui::art::POINTER), theme::accent()),
                Span::styled(file.clone(), theme::accent_bold()),
            ]),
            Row::Problem(index) => {
                let problem = &problems[*index];
                let style = if *index == app.panel.problem_selected {
                    theme::selectable(theme::menu_selected_bg())
                        .fg(severity_color(problem.severity))
                } else {
                    Style::default().fg(severity_color(problem.severity))
                };
                Line::from(Span::styled(format!(" {} ", problem_line(problem)), style))
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

fn problem_line(problem: &ProblemEntry) -> String {
    format!(
        "{} {:>4}:{:<3} {}",
        problem.severity.gutter(),
        problem.position.row + 1,
        problem.position.col + 1,
        problem.message
    )
}

fn render_output(frame: &mut Frame, area: Rect, app: &App) {
    let Some(run) = app.panel.output.as_ref() else {
        render_message(
            frame,
            area,
            "No task output yet.",
            "Run a project task with Ctrl+Shift+R; its output stays here.",
        );
        return;
    };
    if area.height == 0 {
        return;
    }
    let outcome_style = if run.outcome == "running" {
        theme::accent()
    } else if run.success {
        theme::success()
    } else {
        theme::error()
    };
    let header = Line::from(vec![
        Span::styled(format!(" {} ", run.title), theme::accent_bold()),
        Span::styled("·", theme::dim()),
        Span::styled(format!(" {} ", run.command), theme::muted()),
        Span::styled("·", theme::dim()),
        Span::styled(format!(" {} ", run.outcome), outcome_style),
    ]);

    let visible = area.height.saturating_sub(1) as usize;
    let scroll = scroll_for(run.selected, run.lines.len(), visible);
    let lines: Vec<Line> = run
        .lines
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible)
        .map(|(index, line)| {
            let style = if index == run.selected {
                theme::selectable(theme::menu_selected_bg())
            } else {
                theme::text()
            };
            Line::from(Span::styled(format!(" {line}"), style))
        })
        .collect();
    let header_area = Rect { height: 1, ..area };
    frame.render_widget(Paragraph::new(header), header_area);
    let content_area = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    };
    frame.render_widget(Paragraph::new(Text::from(lines)), content_area);
}

fn render_debug_console(frame: &mut Frame, area: Rect, app: &App) {
    if app.panel.debug.is_empty() {
        render_message(
            frame,
            area,
            "The debug console is empty.",
            "Debug Adapter Handshake (Preview) writes its messages here; it is not a debugger yet.",
        );
        return;
    }
    let visible = area.height as usize;
    let selected = app.panel.debug_selected.min(app.panel.debug.len() - 1);
    let scroll = scroll_for(selected, app.panel.debug.len(), visible);
    let lines: Vec<Line> = app
        .panel
        .debug
        .iter()
        .enumerate()
        .skip(scroll)
        .take(visible)
        .map(|(index, message)| {
            let style = if index == selected {
                theme::selectable(theme::menu_selected_bg())
            } else {
                theme::text()
            };
            Line::from(Span::styled(format!(" {message}"), style))
        })
        .collect();
    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

/// A two-line empty state: what is missing, then how to fill it.
fn render_message(frame: &mut Frame, area: Rect, title: &str, hint: &str) {
    let lines = vec![
        Line::from(Span::styled(format!(" {title}"), theme::muted())),
        Line::from(Span::styled(format!(" {hint}"), theme::dim())),
    ];
    frame.render_widget(Paragraph::new(Text::from(lines)), area);
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

fn severity_color(severity: Severity) -> ratatui::style::Color {
    match severity {
        Severity::Error => theme::error_color(),
        Severity::Warning => theme::warn_color(),
        Severity::Info => theme::info_color(),
        Severity::Hint => theme::hint_color(),
    }
}
