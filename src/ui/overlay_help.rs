// Help overlay rendering; included from `overlay.rs`.

/// Render the keyboard-shortcuts cheatsheet.
pub fn render_help(
    frame: &mut Frame,
    area: Rect,
    help: &Help,
    commands: &CommandRegistry,
    phase: usize,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let content = help_lines(commands, phase);
    let width = ((area.width as u32 * 3 / 5) as u16)
        .clamp(40, 76)
        .min(area.width.max(1));
    let height = (content.len() as u16 + 2).min(area.height);
    let rect = centered(area, width, height);

    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme::accent())
        .style(Style::default().bg(theme::panel_bg()))
        .title(Line::from(vec![
            Span::styled("✦ ", theme::star()),
            Span::styled("keyboard shortcuts", theme::accent_bold()),
            Span::styled(" ✦", theme::star()),
        ]));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    if inner.height == 0 {
        return;
    }

    let visible = inner.height as usize;
    let max_scroll = content.len().saturating_sub(visible);
    let scroll = help.scroll.min(max_scroll);
    let slice: Vec<Line> = content.into_iter().skip(scroll).take(visible).collect();
    frame.render_widget(
        Paragraph::new(Text::from(slice)).style(Style::default().bg(theme::panel_bg())),
        inner,
    );
}

/// Build the cheatsheet from the command registry (so it never goes stale) plus
/// a few editor-movement keys that are not commands.
fn help_lines(commands: &CommandRegistry, phase: usize) -> Vec<Line<'static>> {
    let mut order: Vec<&'static str> = Vec::new();
    let mut groups: HashMap<&'static str, Vec<(&'static str, &'static str)>> = HashMap::new();
    for command in commands.all() {
        let Some(shortcut) = command.shortcut else {
            continue;
        };
        if !groups.contains_key(command.category) {
            order.push(command.category);
        }
        groups
            .entry(command.category)
            .or_default()
            .push((shortcut, command.title));
    }

    // Align every shortcut column to the widest entry.
    let key_width = commands
        .all()
        .iter()
        .filter_map(|command| command.shortcut)
        .chain(crate::commands::EDITOR_KEYS.iter().map(|(key, _)| *key))
        .map(|key| key.chars().count())
        .max()
        .unwrap_or(0);

    let mut lines: Vec<Line<'static>> = Vec::new();
    for category in order {
        lines.push(Line::from(Span::styled(
            category.to_string(),
            theme::accent_bold(),
        )));
        for (shortcut, title) in &groups[category] {
            lines.push(help_row(shortcut, title, key_width));
        }
        lines.push(Line::from(""));
    }

    lines.push(Line::from(Span::styled("Editor", theme::accent_bold())));
    for (shortcut, title) in crate::commands::EDITOR_KEYS {
        lines.push(help_row(shortcut, title, key_width));
    }
    lines.push(Line::from(""));

    // A little familiar to keep it warm, blinking on the blink frame.
    let face = if phase % 12 == 7 {
        "( -ω- )"
    } else {
        "( ･ω･ )"
    };
    lines.push(Line::from(vec![
        Span::styled(format!("  {face}  "), theme::soft()),
        Span::styled("Esc to close", theme::muted()),
    ]));
    lines
}

fn help_row(shortcut: &str, title: &str, key_width: usize) -> Line<'static> {
    Line::from(vec![
        Span::styled("  ", theme::dim()),
        Span::styled(format!("{shortcut:<key_width$}"), theme::accent()),
        Span::styled("  ", theme::dim()),
        Span::styled(title.to_string(), theme::text()),
    ])
}

