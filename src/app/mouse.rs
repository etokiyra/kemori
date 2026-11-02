//! Mouse handling for the editor body.
//!
//! Mouse support is deliberately small: the wheel moves the cursor (and
//! therefore the viewport), a left click places it, and a drag selects from the
//! click. Hit-testing lives here rather than in the event loop so it can be
//! tested without a terminal.

use super::*;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

impl App {
    /// Apply one mouse event. Returns whether the frame needs a redraw.
    pub(super) fn handle_mouse(&mut self, mouse: MouseEvent) -> bool {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.with_doc(|doc| doc.move_up(false));
                true
            }
            MouseEventKind::ScrollDown => {
                self.with_doc(|doc| doc.move_down(false));
                true
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let Some(pos) = self.mouse_position(mouse) else {
                    return false;
                };
                self.mouse_drag_anchor = Some(pos);
                self.with_doc(|doc| {
                    doc.selection = None;
                    doc.move_to(pos);
                });
                self.focus = Focus::Editor;
                true
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let (Some(anchor), Some(pos)) =
                    (self.mouse_drag_anchor, self.mouse_position(mouse))
                else {
                    return false;
                };
                self.with_doc(|doc| {
                    doc.selection = Some(Selection::new(anchor));
                    doc.cursor = pos;
                });
                true
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.mouse_drag_anchor = None;
                false
            }
            _ => false,
        }
    }

    /// Map a screen cell inside the editor body to a document position.
    fn mouse_position(&self, mouse: MouseEvent) -> Option<Position> {
        let rect = self.editor_body_rect?;
        if mouse.column < rect.x
            || mouse.column >= rect.x + rect.width
            || mouse.row <= rect.y
            || mouse.row >= rect.y + rect.height
        {
            return None;
        }
        let doc = self.editor.active_document()?;
        let total_lines = doc.buffer.len_lines();
        let number_width = if self.show_line_numbers {
            total_lines.max(1).to_string().len()
        } else {
            0
        };
        let gutter = number_width + 2;
        let row = (mouse.row as usize)
            .saturating_sub(rect.y as usize + 1)
            .saturating_add(doc.scroll_top)
            .min(total_lines.saturating_sub(1));
        let col = (mouse.column as usize)
            .saturating_sub(rect.x as usize + gutter)
            .saturating_add(doc.scroll_left);
        Some(Position::new(row, col))
    }
}
