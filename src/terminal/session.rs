//! PTY-backed terminal sessions.
//!
//! One shell runs in its own pseudo-terminal, so interactive programs see a
//! real terminal (line editing, colour, `isatty`) instead of a pipe. The session
//! keeps the master side, a reader thread forwards output to the UI, and writes
//! and window-size changes go back through the same descriptor.
//!
//! The shell is a child of Kemori but a session leader in its own PTY session,
//! so closing a session signals the whole foreground process group rather than
//! leaking whatever the user started.

use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use super::screen::Screen;

/// A message from the PTY reader thread.
enum Chunk {
    Data(Vec<u8>),
    Exit,
}

/// One shell session: the child, its PTY master and the parsed screen.
pub struct Session {
    id: u64,
    /// What the session is called when the shell has not set a title (the shell
    /// name plus the working directory).
    name: String,
    cwd: PathBuf,
    screen: Screen,
    writer: Arc<Mutex<File>>,
    child: Child,
    pid: i32,
    chunks: Receiver<Chunk>,
    alive: bool,
    /// Lines scrolled up from the bottom; zero follows the tail.
    scroll: usize,
    /// The screen title last observed, so polling can detect a change.
    last_title: Option<String>,
    started: Instant,
}

impl Session {
    /// Start the user's shell in `cwd`.
    pub fn spawn_shell(id: u64, cwd: &Path, cols: usize, rows: usize) -> io::Result<Session> {
        Self::spawn_shell_with(id, cwd, None, cols, rows)
    }

    /// Start `shell` (or the user's `$SHELL`) in `cwd`.
    pub fn spawn_shell_with(
        id: u64,
        cwd: &Path,
        shell: Option<&str>,
        cols: usize,
        rows: usize,
    ) -> io::Result<Session> {
        let shell = shell
            .map(ToString::to_string)
            .or_else(|| std::env::var("SHELL").ok())
            .filter(|shell| !shell.is_empty())
            .unwrap_or_else(|| "/bin/sh".to_string());
        let name = format!("{} — {}", shell_name(&shell), cwd.display());
        Self::spawn_command(id, cwd, &shell, &[], &name, cols, rows)
    }

    /// Start an arbitrary program under a PTY. Used by tests and by the shell
    /// path above; the integrated terminal itself only starts the user's shell.
    pub fn spawn_command(
        id: u64,
        cwd: &Path,
        program: &str,
        args: &[&str],
        name: &str,
        cols: usize,
        rows: usize,
    ) -> io::Result<Session> {
        let (master, slave) = open_pty(cols, rows)?;
        let writer = Arc::new(Mutex::new(master.try_clone()?));
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(cwd)
            .env(
                "TERM",
                std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".into()),
            )
            .env(
                "COLORTERM",
                std::env::var("COLORTERM").unwrap_or_else(|_| "truecolor".into()),
            )
            .stdin(Stdio::from(slave.try_clone()?))
            .stdout(Stdio::from(slave.try_clone()?))
            .stderr(Stdio::from(slave.try_clone()?));
        // A new session makes the child the leader of its own process group;
        // the PTY becomes its controlling terminal.
        let slave_fd = slave.as_raw_fd();
        unsafe {
            command.pre_exec(move || {
                if libc::setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }
                if libc::ioctl(slave_fd, libc::TIOCSCTTY as _, 0) == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn()?;
        // The parent must not keep the slave open, or the master never sees EOF
        // when the shell exits.
        drop(slave);
        let pid = child.id() as i32;

        let (tx, rx) = mpsc::channel();
        let mut reader = master;
        let _ = thread::Builder::new()
            .name(format!("kemori-pty-{id}"))
            .spawn(move || {
                let mut buffer = [0u8; 8192];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => {
                            if tx.send(Chunk::Data(buffer[..n].to_vec())).is_err() {
                                return;
                            }
                        }
                        Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                        Err(_) => break,
                    }
                }
                let _ = tx.send(Chunk::Exit);
            });

        Ok(Session {
            id,
            name: name.to_string(),
            cwd: cwd.to_path_buf(),
            screen: Screen::new(cols, rows),
            writer,
            child,
            pid,
            chunks: rx,
            alive: true,
            scroll: 0,
            last_title: None,
            started: Instant::now(),
        })
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn screen(&self) -> &Screen {
        &self.screen
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }

    pub fn is_alive(&self) -> bool {
        self.alive
    }

    /// Seconds since the session started, for the header.
    pub fn uptime_secs(&self) -> u64 {
        self.started.elapsed().as_secs()
    }

    /// The title set by the running program, or the session's fallback name.
    pub fn title(&self) -> String {
        self.screen
            .title()
            .map(ToString::to_string)
            .unwrap_or_else(|| self.name.clone())
    }

    /// Send bytes to the shell.
    pub fn write(&mut self, bytes: &[u8]) -> bool {
        if !self.alive {
            return false;
        }
        let result = match self.writer.lock() {
            Ok(mut writer) => writer.write_all(bytes).and_then(|_| writer.flush()),
            Err(poisoned) => {
                let mut writer = poisoned.into_inner();
                writer.write_all(bytes).and_then(|_| writer.flush())
            }
        };
        if result.is_err() {
            self.alive = false;
        }
        result.is_ok()
    }

    /// Resize both the screen model and the kernel PTY.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        if cols == self.screen.cols() && rows == self.screen.rows() {
            return;
        }
        self.screen.resize(cols, rows);
        self.scroll = self.scroll.min(self.screen.scrollback_len());
        let size = libc::winsize {
            ws_row: rows.min(u16::MAX as usize) as u16,
            ws_col: cols.min(u16::MAX as usize) as u16,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        if let Ok(writer) = self.writer.lock() {
            let fd = writer.as_raw_fd();
            // The kernel normally signals SIGWINCH on a size change; sending it
            // explicitly covers PTY implementations that do not.
            unsafe {
                let _ = libc::ioctl(fd, libc::TIOCSWINSZ as _, &size);
                let _ = libc::kill(-self.pid, libc::SIGWINCH);
            }
        }
    }

    /// Drain output and update the alive state. Returns whether the view changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.chunks.try_recv() {
                Ok(Chunk::Data(bytes)) => {
                    self.screen.advance(&bytes);
                    changed = true;
                }
                Ok(Chunk::Exit) => {
                    self.alive = false;
                    changed = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.alive = false;
                    break;
                }
            }
        }
        if self.alive
            && let Ok(Some(_status)) = self.child.try_wait()
        {
            self.alive = false;
            changed = true;
        }
        if self.screen.title() != self.last_title.as_deref() {
            self.last_title = self.screen.title().map(ToString::to_string);
            changed = true;
        }
        if self.scroll > self.screen.scrollback_len() {
            self.scroll = self.screen.scrollback_len();
            changed = true;
        }
        changed
    }

    /// Scroll up (`delta > 0`) or down through the scrollback.
    pub fn scroll_by(&mut self, delta: isize) {
        let max = self.screen.scrollback_len();
        let next = self.scroll as isize + delta;
        self.scroll = next.clamp(0, max as isize) as usize;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll = 0;
    }

    /// Terminate the shell and everything in its process group.
    pub fn terminate(&mut self) {
        if !self.alive {
            let _ = self.child.wait();
            return;
        }
        unsafe {
            // The child is the session leader, so its pid is the process-group
            // id; a negative pid signals the whole group.
            let _ = libc::kill(-self.pid, libc::SIGHUP);
            let _ = libc::kill(-self.pid, libc::SIGTERM);
            let _ = libc::kill(-self.pid, libc::SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.alive = false;
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.terminate();
    }
}

/// Every terminal session in this Kemori instance.
pub struct Terminals {
    sessions: Vec<Session>,
    active: usize,
    next_id: u64,
}

impl Default for Terminals {
    fn default() -> Self {
        Terminals {
            sessions: Vec::new(),
            active: 0,
            next_id: 1,
        }
    }
}

impl Terminals {
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn sessions(&self) -> &[Session] {
        &self.sessions
    }

    pub fn active(&self) -> Option<&Session> {
        self.sessions.get(self.active)
    }

    pub fn active_mut(&mut self) -> Option<&mut Session> {
        self.sessions.get_mut(self.active)
    }

    /// Start a shell rooted at `cwd` and make it the active session.
    pub fn spawn_shell(&mut self, cwd: &Path, cols: usize, rows: usize) -> io::Result<usize> {
        self.spawn_shell_with(cwd, None, cols, rows)
    }

    /// Start `shell` (or the user's `$SHELL`) rooted at `cwd`.
    pub fn spawn_shell_with(
        &mut self,
        cwd: &Path,
        shell: Option<&str>,
        cols: usize,
        rows: usize,
    ) -> io::Result<usize> {
        let id = self.next_id;
        self.next_id += 1;
        let session = Session::spawn_shell_with(id, cwd, shell, cols, rows)?;
        self.sessions.push(session);
        self.active = self.sessions.len() - 1;
        Ok(self.active)
    }

    /// Start an explicit program under a PTY. The integrated terminal always
    /// uses the user's shell; this entry point exists for tests and for a future
    /// "run this command in a terminal" action.
    #[allow(dead_code)]
    pub fn spawn_command(
        &mut self,
        cwd: &Path,
        program: &str,
        args: &[&str],
        name: &str,
        cols: usize,
        rows: usize,
    ) -> io::Result<usize> {
        let id = self.next_id;
        self.next_id += 1;
        let session = Session::spawn_command(id, cwd, program, args, name, cols, rows)?;
        self.sessions.push(session);
        self.active = self.sessions.len() - 1;
        Ok(self.active)
    }

    /// Select a session by index, clamped to the available sessions.
    pub fn select(&mut self, index: usize) {
        if !self.sessions.is_empty() {
            self.active = index.min(self.sessions.len() - 1);
        }
    }

    /// Select the next session, wrapping around.
    pub fn next(&mut self) {
        if !self.sessions.is_empty() {
            self.active = (self.active + 1) % self.sessions.len();
        }
    }

    /// Close the active session and choose a neighbour.
    pub fn close_active(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        let mut session = self.sessions.remove(self.active);
        session.terminate();
        if self.active >= self.sessions.len() {
            self.active = self.sessions.len().saturating_sub(1);
        }
    }

    pub fn write_active(&mut self, bytes: &[u8]) -> bool {
        self.active_mut()
            .is_some_and(|session| session.write(bytes))
    }

    pub fn scroll_active(&mut self, delta: isize) {
        if let Some(session) = self.active_mut() {
            session.scroll_by(delta);
        }
    }

    pub fn scroll_active_to_bottom(&mut self) {
        if let Some(session) = self.active_mut() {
            session.scroll_to_bottom();
        }
    }

    /// Drain every session's output. Returns whether anything changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        for session in &mut self.sessions {
            changed |= session.poll();
        }
        changed
    }

    /// Resize the active session's screen and PTY.
    pub fn resize_active(&mut self, cols: usize, rows: usize) {
        if let Some(session) = self.active_mut() {
            session.resize(cols, rows);
        }
    }

    /// Close every session, used when Kemori exits.
    pub fn kill_all(&mut self) {
        for mut session in self.sessions.drain(..) {
            session.terminate();
        }
        self.active = 0;
    }
}

impl Drop for Terminals {
    fn drop(&mut self) {
        self.kill_all();
    }
}

/// Allocate a master/slave PTY pair at the given size.
fn open_pty(cols: usize, rows: usize) -> io::Result<(File, File)> {
    let mut master = 0;
    let mut slave = 0;
    let size = libc::winsize {
        ws_row: rows.max(1).min(u16::MAX as usize) as u16,
        ws_col: cols.max(1).min(u16::MAX as usize) as u16,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let result = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &size,
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    // `openpty` returned owned descriptors; take them into `File` so they are
    // closed exactly once.
    let master = unsafe { File::from_raw_fd(master) };
    let slave = unsafe { File::from_raw_fd(slave) };
    Ok((master, slave))
}

/// The final path component of a shell path.
fn shell_name(shell: &str) -> String {
    Path::new(shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(shell)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn runs_a_command_in_a_pty_and_reads_its_output() {
        let cwd = std::env::temp_dir();
        let mut session = Session::spawn_command(
            1,
            &cwd,
            "/bin/sh",
            &["-c", "printf 'hello\\n'"],
            "test",
            40,
            8,
        )
        .expect("spawn a pty");
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && !session.screen().plain_text().contains("hello") {
            session.poll();
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            session.screen().plain_text().contains("hello"),
            "output was {:?}",
            session.screen().plain_text()
        );
    }

    #[test]
    fn writes_input_to_the_shell() {
        let cwd = std::env::temp_dir();
        let mut session =
            Session::spawn_command(2, &cwd, "/bin/sh", &[], "test", 60, 10).expect("spawn a pty");
        session.write(b"echo terminal-ok\n");
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && !session.screen().plain_text().contains("terminal-ok") {
            session.poll();
            thread::sleep(Duration::from_millis(10));
        }
        assert!(session.screen().plain_text().contains("terminal-ok"));
        session.terminate();
    }
}
