//! The main event loop, extracted from `app/mod.rs` to keep the state module
//! under the project's file-size limit. See `docs/DECISIONS.md`.

use super::*;

impl App {
    pub(super) fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        let mut needs_redraw = true;
        while !self.should_quit {
            // Only repaint when something changed: input arrived, background
            // work completed, the animation advanced, a status message expired,
            // or the terminal was resized. An idle Kemori does no work at all.
            self.poll_diagnostics();
            self.poll_lsp_start();
            let lsp_health_changed = self.poll_lsp_health();
            let lsp_changed = self.poll_lsp() || lsp_health_changed;
            let completion_changed = self.poll_auto_completion();
            let background_changed = self.apply_background_events();
            let animated = self.tick_animation();
            let external_changed = self.poll_external_changes();
            // Scan the project's languages once, after the user has engaged with
            // a project, so setup can be offered without blocking startup.
            if self.engaged && !self.project_scan_started {
                self.project_scan_started = true;
                self.background
                    .detect_project_languages(self.workspace.root().to_path_buf());
            }
            let offered = self.maybe_offer_project_setup() || self.maybe_offer_tool_setup();
            if needs_redraw
                || background_changed
                || lsp_changed
                || completion_changed
                || animated
                || external_changed
                || offered
                || self.tick_status()
            {
                terminal.draw(|frame| ui::render(frame, self))?;
                needs_redraw = false;
            }
            let timeout = if self.wants_animation() {
                // Wake exactly when the next frame is due, not sooner.
                self.animation_interval()
                    .saturating_sub(self.anim_last.elapsed())
                    .max(Duration::from_millis(16))
            } else {
                Duration::from_millis(250)
            };
            // Wake early when an automatic completion is due, so the popup is
            // not delayed by the idle timeout.
            let timeout = match self.completion_due.as_ref() {
                Some(pending) => {
                    timeout.min(pending.due_at.saturating_duration_since(Instant::now()))
                }
                None => timeout,
            };
            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key)
                        if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
                    {
                        self.handle_key(key);
                        needs_redraw = true;
                    }
                    Event::Paste(text) => {
                        self.handle_paste(&text);
                        needs_redraw = true;
                    }
                    Event::FocusGained => {
                        // Files may have appeared or vanished while Kemori was in
                        // the background; catch up quietly.
                        self.workspace.refresh();
                        self.request_git_refresh();
                        needs_redraw = true;
                    }
                    Event::Resize(..) => needs_redraw = true,
                    Event::Mouse(mouse) if self.handle_mouse(mouse) => {
                        needs_redraw = true;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}
