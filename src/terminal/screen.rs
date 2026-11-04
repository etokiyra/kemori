//! A small terminal screen for the integrated terminal.
//!
//! A PTY is a byte stream; the panel needs a *screen*. This module is a
//! deliberately modest ANSI interpreter: it understands the cursor movement,
//! erase, scroll-region and alternate-screen sequences a shell, `less`, `vim`
//! and most command-line tools use, keeps a bounded scrollback, and ignores
//! colour attributes (Kemori draws terminal text with the theme's text role so
//! a transparent terminal keeps working). It is not a complete VT100.
//!
//! The parser accepts bytes split at arbitrary boundaries, so a multi-byte UTF-8
//! character or an escape sequence may arrive in several PTY reads.

/// How many lines of scrollback a session keeps. Bounded so a chatty process
/// cannot grow memory without limit.
pub const SCROLLBACK_LIMIT: usize = 2_000;

/// One character cell. Colour is intentionally not stored; see the module note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cell {
    ch: char,
}

impl Default for Cell {
    fn default() -> Self {
        Cell { ch: ' ' }
    }
}

#[derive(Clone, Debug)]
enum Parser {
    Ground,
    Escape,
    Csi { params: Vec<u8>, private: bool },
    Osc { data: String },
    OscEscape { data: String },
}

/// A parsed screen with a cursor, a scroll region and bounded scrollback.
pub struct Screen {
    cols: usize,
    rows: usize,
    lines: Vec<Vec<Cell>>,
    scrollback: Vec<Vec<Cell>>,
    cursor_row: usize,
    cursor_col: usize,
    scroll_top: usize,
    scroll_bottom: usize,
    wrap_pending: bool,
    saved_cursor: (usize, usize),
    /// Saved contents while an application is on the alternate screen.
    alt_saved: Option<(Vec<Vec<Cell>>, usize, usize)>,
    parser: Parser,
    pending: Vec<u8>,
    title: Option<String>,
}

impl Screen {
    /// Create a blank screen. Both dimensions are clamped to at least one cell.
    pub fn new(cols: usize, rows: usize) -> Screen {
        let cols = cols.max(1);
        let rows = rows.max(1);
        Screen {
            cols,
            rows,
            lines: vec![vec![Cell::default(); cols]; rows],
            scrollback: Vec::new(),
            cursor_row: 0,
            cursor_col: 0,
            scroll_top: 0,
            scroll_bottom: rows - 1,
            wrap_pending: false,
            saved_cursor: (0, 0),
            alt_saved: None,
            parser: Parser::Ground,
            pending: Vec::new(),
            title: None,
        }
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    /// The title an application set through OSC 0/2, if any.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// How many lines of scrollback are available.
    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    /// Resize the screen, keeping the top-left content and clamping the cursor.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if cols == self.cols && rows == self.rows {
            return;
        }
        for line in &mut self.lines {
            line.resize(cols, Cell::default());
        }
        if rows < self.lines.len() {
            self.lines.truncate(rows);
        } else {
            self.lines.resize_with(rows, || vec![Cell::default(); cols]);
        }
        // A wider screen may leave the cursor inside the new area; a shorter
        // one must not point past the end.
        self.cursor_col = self.cursor_col.min(cols - 1);
        self.cursor_row = self.cursor_row.min(rows - 1);
        self.scroll_top = 0;
        self.scroll_bottom = rows - 1;
        self.wrap_pending = false;
        self.cols = cols;
        self.rows = rows;
    }

    /// Feed raw PTY bytes. Incomplete UTF-8 is held until the next read.
    pub fn advance(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        let mut consumed = 0;
        loop {
            let (text, length, incomplete, invalid) =
                match std::str::from_utf8(&self.pending[consumed..]) {
                    Ok(text) => (text.to_string(), text.len(), true, false),
                    Err(err) => {
                        let valid = err.valid_up_to();
                        let text = std::str::from_utf8(
                            &self.pending[consumed..consumed.saturating_add(valid)],
                        )
                        .unwrap_or("")
                        .to_string();
                        let invalid = err.error_len().is_some();
                        let length = valid + usize::from(invalid);
                        (text, length, !invalid, invalid)
                    }
                };
            for ch in text.chars() {
                self.feed_char(ch);
            }
            if invalid {
                // A genuinely invalid byte: show a replacement and move on
                // instead of stalling the stream forever.
                self.put_char('\u{fffd}');
            }
            consumed += length;
            if incomplete {
                break;
            }
        }
        self.pending.drain(..consumed);
    }

    /// The visible rows, `scroll` lines up from the bottom.
    pub fn view(&self, scroll: usize, height: usize) -> Vec<String> {
        let total = self.scrollback.len() + self.lines.len();
        let height = height.max(1).min(total.max(1));
        let end = total.saturating_sub(scroll.min(total));
        let start = end.saturating_sub(height);
        (start..end)
            .map(|index| {
                let line = if index < self.scrollback.len() {
                    &self.scrollback[index]
                } else {
                    &self.lines[index - self.scrollback.len()]
                };
                let text: String = line.iter().map(|cell| cell.ch).collect();
                text.trim_end().to_string()
            })
            .collect()
    }

    /// Where the cursor is inside a [`Screen::view`], when it is on screen.
    pub fn cursor_in_view(&self, scroll: usize, height: usize) -> Option<(usize, usize)> {
        if scroll > 0 {
            return None;
        }
        let total = self.scrollback.len() + self.lines.len();
        let height = height.max(1).min(total.max(1));
        let start = total.saturating_sub(height);
        let absolute = self.scrollback.len() + self.cursor_row;
        if absolute < start || absolute >= total {
            return None;
        }
        Some((absolute - start, self.cursor_col.min(self.cols - 1)))
    }

    /// The whole screen as plain text, for the clipboard.
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        for line in self.scrollback.iter().chain(self.lines.iter()) {
            let text: String = line.iter().map(|cell| cell.ch).collect();
            out.push_str(text.trim_end());
            out.push('\n');
        }
        out
    }

    fn feed_char(&mut self, ch: char) {
        let parser = std::mem::replace(&mut self.parser, Parser::Ground);
        match parser {
            Parser::Ground => match ch {
                '\x1b' => self.parser = Parser::Escape,
                '\x07' => {}
                '\x08' => self.backspace(),
                '\t' => self.tab(),
                '\n' | '\x0b' | '\x0c' => self.linefeed(),
                '\r' => {
                    self.cursor_col = 0;
                    self.wrap_pending = false;
                }
                ch if (ch as u32) < 0x20 => {}
                ch => self.put_char(ch),
            },
            Parser::Escape => match ch {
                '[' => {
                    self.parser = Parser::Csi {
                        params: Vec::new(),
                        private: false,
                    }
                }
                ']' => {
                    self.parser = Parser::Osc {
                        data: String::new(),
                    }
                }
                '7' => self.saved_cursor = (self.cursor_row, self.cursor_col),
                '8' => {
                    self.cursor_row = self.saved_cursor.0.min(self.rows - 1);
                    self.cursor_col = self.saved_cursor.1.min(self.cols - 1);
                }
                'D' => self.linefeed(),
                'M' => self.reverse_index(),
                'E' => {
                    self.cursor_col = 0;
                    self.linefeed();
                }
                'c' => self.reset(),
                _ => {}
            },
            Parser::Csi {
                mut params,
                mut private,
            } => {
                if (ch as u32) >= 0x40 && (ch as u32) <= 0x7e {
                    self.execute_csi(&params, private, ch);
                } else if ch == '?' && params.is_empty() {
                    private = true;
                    self.parser = Parser::Csi { params, private };
                } else if (ch as u32) >= 0x20 && params.len() < 64 {
                    params.push(ch as u8);
                    self.parser = Parser::Csi { params, private };
                } else {
                    self.parser = Parser::Csi { params, private };
                }
            }
            Parser::Osc { mut data } => {
                if ch == '\x07' {
                    self.finish_osc(&data);
                } else if ch == '\x1b' {
                    // ST is ESC `\`; keep the payload until the next byte.
                    self.parser = Parser::OscEscape { data };
                } else if data.len() < 512 {
                    data.push(ch);
                    self.parser = Parser::Osc { data };
                } else {
                    self.parser = Parser::Osc { data };
                }
            }
            Parser::OscEscape { data } => {
                if ch == '\\' {
                    self.finish_osc(&data);
                } else {
                    self.parser = Parser::Osc { data };
                    self.feed_char(ch);
                }
            }
        }
    }

    fn finish_osc(&mut self, data: &str) {
        if let Some((kind, value)) = data.split_once(';')
            && (kind == "0" || kind == "2")
            && !value.is_empty()
        {
            self.title = Some(value.to_string());
        }
        self.parser = Parser::Ground;
    }

    fn put_char(&mut self, ch: char) {
        if self.wrap_pending {
            self.cursor_col = 0;
            self.linefeed();
        }
        let row = self.cursor_row.min(self.rows - 1);
        let col = self.cursor_col.min(self.cols - 1);
        if let Some(line) = self.lines.get_mut(row)
            && let Some(cell) = line.get_mut(col)
        {
            *cell = Cell { ch };
        }
        if self.cursor_col + 1 >= self.cols {
            self.cursor_col = self.cols - 1;
            self.wrap_pending = true;
        } else {
            self.cursor_col += 1;
            self.wrap_pending = false;
        }
    }

    fn backspace(&mut self) {
        if self.wrap_pending {
            self.wrap_pending = false;
        } else if self.cursor_col > 0 {
            self.cursor_col -= 1;
        }
    }

    fn tab(&mut self) {
        let next = ((self.cursor_col / 8) + 1) * 8;
        self.cursor_col = next.min(self.cols.saturating_sub(1));
        self.wrap_pending = false;
    }

    fn linefeed(&mut self) {
        self.wrap_pending = false;
        if self.cursor_row == self.scroll_bottom || self.cursor_row >= self.rows - 1 {
            self.scroll_up(1);
        } else {
            self.cursor_row += 1;
        }
    }

    fn reverse_index(&mut self) {
        self.wrap_pending = false;
        if self.cursor_row == self.scroll_top {
            self.scroll_down(1);
        } else {
            self.cursor_row = self.cursor_row.saturating_sub(1);
        }
    }

    fn scroll_up(&mut self, count: usize) {
        let top = self.scroll_top.min(self.rows - 1);
        let bottom = self.scroll_bottom.min(self.rows - 1).max(top);
        for _ in 0..count {
            let line = self.lines.remove(top);
            if top == 0 && bottom == self.rows - 1 {
                self.scrollback.push(line);
                if self.scrollback.len() > SCROLLBACK_LIMIT {
                    self.scrollback.remove(0);
                }
            }
            self.lines.insert(bottom, vec![Cell::default(); self.cols]);
        }
    }

    fn scroll_down(&mut self, count: usize) {
        let top = self.scroll_top.min(self.rows - 1);
        let bottom = self.scroll_bottom.min(self.rows - 1).max(top);
        for _ in 0..count {
            self.lines.remove(bottom);
            self.lines.insert(top, vec![Cell::default(); self.cols]);
        }
    }

    fn reset(&mut self) {
        for line in &mut self.lines {
            line.fill(Cell::default());
        }
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.scroll_top = 0;
        self.scroll_bottom = self.rows - 1;
        self.wrap_pending = false;
    }

    fn execute_csi(&mut self, raw: &[u8], private: bool, final_char: char) {
        let text = String::from_utf8_lossy(raw);
        let intermediate: String = text
            .chars()
            .filter(|ch| !ch.is_ascii_digit() && *ch != ';' && *ch != ':')
            .collect();
        let numbers: Vec<u32> = text
            .split(';')
            .map(|part| {
                part.chars()
                    .take_while(|ch| ch.is_ascii_digit())
                    .collect::<String>()
                    .parse::<u32>()
                    .unwrap_or(0)
            })
            .collect();
        let param = |index: usize, default: u32| -> u32 {
            numbers
                .get(index)
                .copied()
                .filter(|value| *value != 0)
                .unwrap_or(default)
        };
        if !intermediate.is_empty() && final_char != 'm' {
            return;
        }
        match final_char {
            'A' => {
                self.cursor_row = self.cursor_row.saturating_sub(param(0, 1) as usize);
                self.wrap_pending = false;
            }
            'B' | 'e' => {
                let bottom = self.scroll_bottom.min(self.rows - 1);
                self.cursor_row = (self.cursor_row + param(0, 1) as usize).min(bottom);
                self.wrap_pending = false;
            }
            'C' | 'a' => {
                self.cursor_col = (self.cursor_col + param(0, 1) as usize).min(self.cols - 1);
                self.wrap_pending = false;
            }
            'D' => {
                self.cursor_col = self.cursor_col.saturating_sub(param(0, 1) as usize);
                self.wrap_pending = false;
            }
            'E' => {
                let bottom = self.scroll_bottom.min(self.rows - 1);
                self.cursor_row = (self.cursor_row + param(0, 1) as usize).min(bottom);
                self.cursor_col = 0;
                self.wrap_pending = false;
            }
            'F' => {
                self.cursor_row = self
                    .cursor_row
                    .saturating_sub(param(0, 1) as usize)
                    .max(self.scroll_top);
                self.cursor_col = 0;
                self.wrap_pending = false;
            }
            'G' | '`' => {
                self.cursor_col = (param(0, 1) as usize - 1).min(self.cols - 1);
                self.wrap_pending = false;
            }
            'd' => {
                self.cursor_row = (param(0, 1) as usize - 1).min(self.rows - 1);
                self.wrap_pending = false;
            }
            'H' | 'f' => {
                self.cursor_row = (param(0, 1) as usize - 1).min(self.rows - 1);
                self.cursor_col = (param(1, 1) as usize - 1).min(self.cols - 1);
                self.wrap_pending = false;
            }
            'J' => self.erase_display(param(0, 0)),
            'K' => self.erase_line(param(0, 0)),
            'L' => {
                let count = param(0, 1) as usize;
                let row = self.cursor_row.min(self.rows - 1);
                let bottom = self.scroll_bottom.min(self.rows - 1);
                for _ in 0..count {
                    self.lines.remove(bottom);
                    self.lines.insert(row, vec![Cell::default(); self.cols]);
                }
            }
            'M' => {
                let count = param(0, 1) as usize;
                let row = self.cursor_row.min(self.rows - 1);
                let bottom = self.scroll_bottom.min(self.rows - 1);
                for _ in 0..count {
                    self.lines.remove(row);
                    self.lines.insert(bottom, vec![Cell::default(); self.cols]);
                }
            }
            'P' => {
                let count = param(0, 1) as usize;
                let row = self.cursor_row.min(self.rows - 1);
                let col = self.cursor_col.min(self.cols - 1);
                if let Some(line) = self.lines.get_mut(row) {
                    let take = count.min(line.len().saturating_sub(col));
                    line.drain(col..col + take);
                    line.resize(self.cols, Cell::default());
                }
            }
            '@' => {
                let count = param(0, 1) as usize;
                let row = self.cursor_row.min(self.rows - 1);
                let col = self.cursor_col.min(self.cols - 1);
                if let Some(line) = self.lines.get_mut(row) {
                    let take = count.min(self.cols.saturating_sub(col));
                    for _ in 0..take {
                        line.insert(col, Cell::default());
                    }
                    line.truncate(self.cols);
                }
            }
            'X' => {
                let count = param(0, 1) as usize;
                let row = self.cursor_row.min(self.rows - 1);
                let col = self.cursor_col.min(self.cols - 1);
                if let Some(line) = self.lines.get_mut(row) {
                    let end = (col + count).min(line.len());
                    line[col..end].fill(Cell::default());
                }
            }
            'S' => self.scroll_up(param(0, 1) as usize),
            'T' => self.scroll_down(param(0, 1) as usize),
            'r' => {
                let top = (param(0, 1) as usize - 1).min(self.rows - 1);
                let bottom = (param(1, self.rows as u32) as usize - 1).min(self.rows - 1);
                if top < bottom {
                    self.scroll_top = top;
                    self.scroll_bottom = bottom;
                    self.cursor_row = top;
                    self.cursor_col = 0;
                }
            }
            's' => self.saved_cursor = (self.cursor_row, self.cursor_col),
            'u' => {
                self.cursor_row = self.saved_cursor.0.min(self.rows - 1);
                self.cursor_col = self.saved_cursor.1.min(self.cols - 1);
            }
            'h' | 'l' if private => {
                for mode in &numbers {
                    match (*mode, final_char) {
                        (1049 | 1047 | 47, 'h') => self.enter_alt_screen(),
                        (1049 | 1047 | 47, 'l') => self.leave_alt_screen(),
                        (1048, 'h') => self.saved_cursor = (self.cursor_row, self.cursor_col),
                        (1048, 'l') => {
                            self.cursor_row = self.saved_cursor.0.min(self.rows - 1);
                            self.cursor_col = self.saved_cursor.1.min(self.cols - 1);
                        }
                        _ => {}
                    }
                }
            }
            // SGR, device reports, window operations, keyboard modes and the
            // rest are accepted and ignored: text stays readable in the theme.
            _ => {}
        }
    }

    fn enter_alt_screen(&mut self) {
        if self.alt_saved.is_some() {
            return;
        }
        self.alt_saved = Some((self.lines.clone(), self.cursor_row, self.cursor_col));
        for line in &mut self.lines {
            line.fill(Cell::default());
        }
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.wrap_pending = false;
    }

    fn leave_alt_screen(&mut self) {
        if let Some((lines, row, col)) = self.alt_saved.take() {
            if lines.len() == self.rows {
                self.lines = lines;
            }
            self.cursor_row = row.min(self.rows - 1);
            self.cursor_col = col.min(self.cols - 1);
            self.wrap_pending = false;
        }
    }

    fn erase_display(&mut self, mode: u32) {
        match mode {
            0 => {
                self.erase_line(0);
                let row = self.cursor_row.min(self.rows - 1);
                for line in self.lines.iter_mut().skip(row + 1) {
                    line.fill(Cell::default());
                }
            }
            1 => {
                self.erase_line(1);
                let row = self.cursor_row.min(self.rows - 1);
                for line in self.lines.iter_mut().take(row) {
                    line.fill(Cell::default());
                }
            }
            _ => {
                for line in &mut self.lines {
                    line.fill(Cell::default());
                }
                if mode == 3 {
                    self.scrollback.clear();
                }
            }
        }
    }

    fn erase_line(&mut self, mode: u32) {
        let row = self.cursor_row.min(self.rows - 1);
        let col = self.cursor_col.min(self.cols - 1);
        let Some(line) = self.lines.get_mut(row) else {
            return;
        };
        match mode {
            0 => line[col..].fill(Cell::default()),
            1 => line[..=col].fill(Cell::default()),
            _ => line.fill(Cell::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(screen: &Screen) -> Vec<String> {
        screen.view(0, screen.rows())
    }

    #[test]
    fn writes_text_and_wraps_at_the_edge() {
        let mut screen = Screen::new(5, 2);
        screen.advance(b"abcdef");
        // The sixth character wrapped to the next row because the fifth filled
        // the first row (with a pending-wrap cursor at its edge).
        assert_eq!(text(&screen), vec!["abcde", "f"]);
    }

    #[test]
    fn carriage_return_and_linefeed_move_the_cursor() {
        let mut screen = Screen::new(10, 2);
        screen.advance(b"hello\rHI\r\nworld");
        assert_eq!(text(&screen)[0], "HIllo");
        assert_eq!(text(&screen)[1], "world");
    }

    #[test]
    fn cursor_positioning_and_erase() {
        let mut screen = Screen::new(10, 3);
        screen.advance(b"one\r\ntwo\r\nthree");
        // Move to row 2, column 1 and erase to the end of the line.
        screen.advance(b"\x1b[2;1H\x1b[K");
        assert_eq!(text(&screen), vec!["one", "", "three"]);
    }

    #[test]
    fn scrolls_at_the_bottom_and_keeps_scrollback() {
        let mut screen = Screen::new(10, 2);
        screen.advance(b"one\r\ntwo\r\nthree\r\nfour");
        assert_eq!(text(&screen), vec!["three", "four"]);
        assert_eq!(screen.scrollback_len(), 2);
        assert_eq!(screen.view(1, 2), vec!["two", "three"]);
    }

    #[test]
    fn alternate_screen_restores_the_shell_screen() {
        let mut screen = Screen::new(10, 2);
        screen.advance(b"shell");
        screen.advance(b"\x1b[?1049h");
        assert_eq!(text(&screen), vec!["", ""]);
        screen.advance(b"pager");
        screen.advance(b"\x1b[?1049l");
        assert_eq!(text(&screen)[0], "shell");
    }

    #[test]
    fn reads_an_osc_title() {
        let mut screen = Screen::new(10, 2);
        screen.advance(b"\x1b]0;my project\x07");
        assert_eq!(screen.title(), Some("my project"));
    }

    #[test]
    fn handles_utf8_split_between_reads() {
        let mut screen = Screen::new(10, 2);
        let bytes = "héllo".as_bytes();
        screen.advance(&bytes[..2]);
        assert_eq!(text(&screen)[0], "h");
        screen.advance(&bytes[2..]);
        assert_eq!(text(&screen)[0], "héllo");
    }

    #[test]
    fn cursor_is_hidden_when_scrolled_back() {
        let mut screen = Screen::new(10, 2);
        screen.advance(b"a\r\nb\r\nc");
        assert!(screen.cursor_in_view(0, 2).is_some());
        assert!(screen.cursor_in_view(1, 2).is_none());
    }
}
