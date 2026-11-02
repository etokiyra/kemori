//! Bounded, timed subprocess capture.
//!
//! External tools (formatters, package managers, `git`) are run as children and
//! must never be able to hang Kemori forever or exhaust memory with unbounded
//! output. This module captures both pipes on their own threads — so a child
//! cannot deadlock on a full pipe — caps the bytes kept, and kills the child if
//! it runs past a deadline.

use std::io::{self, Read};
use std::process::{Child, ExitStatus};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// How often the child is polled while waiting for it to exit.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// A chunk callback shared by the two reader threads.
type ChunkHandler = Arc<dyn Fn(Stream, &str) + Send + Sync>;

/// Which pipe a streamed chunk came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// The result of a streamed run, with the cancellation state separated from a
/// timeout so the caller can explain what happened.
pub struct Streamed {
    pub captured: Captured,
    pub cancelled: bool,
}

/// The captured result of a child process.
pub struct Captured {
    /// The exit status, or `None` when the child had to be killed because it
    /// ran past the timeout.
    pub status: Option<ExitStatus>,
    /// Up to the caller's cap of stdout, with the rest drained and discarded.
    pub stdout: Vec<u8>,
    /// Up to the caller's cap of stderr, with the rest drained and discarded.
    pub stderr: Vec<u8>,
}

/// Wait for `child`, capturing bounded stdout/stderr and killing it if it runs
/// past `timeout`.
///
/// The child's stdout and stderr must be piped before calling. Returns an error
/// only if waiting itself fails; a timeout is reported as `status == None`.
pub fn wait_captured(
    child: &mut Child,
    timeout: Duration,
    max_output: usize,
) -> io::Result<Captured> {
    let stdout = child
        .stdout
        .take()
        .map(|pipe| thread::spawn(move || read_capped(pipe, max_output)));
    let stderr = child
        .stderr
        .take()
        .map(|pipe| thread::spawn(move || read_capped(pipe, max_output)));

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait()? {
            Some(status) => break Some(status),
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                thread::sleep(POLL_INTERVAL);
            }
        }
    };

    let stdout = stdout
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    let stderr = stderr
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    Ok(Captured {
        status,
        stdout,
        stderr,
    })
}

/// Wait for `child`, forwarding stdout/stderr chunks while capturing the same
/// bounded output as [`wait_captured`].
///
/// The callback runs on the two reader threads, so it must be `Sync`; a
/// callback that sends to a channel is the intended shape. `cancel` is checked
/// on every poll, and a set flag kills the child and reports `cancelled`.
pub fn wait_streaming<F>(
    child: &mut Child,
    timeout: Duration,
    max_output: usize,
    cancel: &AtomicBool,
    on_chunk: F,
) -> io::Result<Streamed>
where
    F: Fn(Stream, &str) + Send + Sync + 'static,
{
    let callback: ChunkHandler = Arc::new(on_chunk);
    let stdout = child.stdout.take().map(|pipe| {
        let callback = Arc::clone(&callback);
        thread::spawn(move || read_stream(pipe, max_output, Stream::Stdout, callback))
    });
    let stderr = child.stderr.take().map(|pipe| {
        let callback = Arc::clone(&callback);
        thread::spawn(move || read_stream(pipe, max_output, Stream::Stderr, callback))
    });

    let deadline = Instant::now() + timeout;
    let mut cancelled = false;
    let status = loop {
        match child.try_wait()? {
            Some(status) => break Some(status),
            None => {
                if cancel.load(Ordering::Relaxed) {
                    let _ = child.kill();
                    let _ = child.wait();
                    cancelled = true;
                    break None;
                }
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                thread::sleep(POLL_INTERVAL);
            }
        }
    };

    let stdout = stdout
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    let stderr = stderr
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    Ok(Streamed {
        captured: Captured {
            status,
            stdout,
            stderr,
        },
        cancelled,
    })
}

/// Read a pipe, keeping and forwarding up to `max` bytes, then draining.
fn read_stream<R: Read>(
    mut reader: R,
    max: usize,
    stream: Stream,
    on_chunk: ChunkHandler,
) -> Vec<u8> {
    let mut kept = Vec::new();
    let mut pending = Vec::new();
    let mut forwarded = 0usize;
    let mut chunk = [0u8; 8192];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                if kept.len() < max {
                    let take = (max - kept.len()).min(n);
                    kept.extend_from_slice(&chunk[..take]);
                }
                if forwarded >= max {
                    continue;
                }
                pending.extend_from_slice(&chunk[..n]);
                forwarded += emit_utf8(&mut pending, stream, &on_chunk);
            }
            Err(_) => break,
        }
    }
    // A final incomplete sequence is shown as a replacement rather than lost.
    if !pending.is_empty() {
        on_chunk(stream, &String::from_utf8_lossy(&pending));
    }
    kept
}

/// Emit every complete UTF-8 sequence in `pending`, keeping an incomplete tail
/// for the next read. Returns how many bytes were forwarded.
fn emit_utf8(pending: &mut Vec<u8>, stream: Stream, on_chunk: &ChunkHandler) -> usize {
    let mut forwarded = 0;
    loop {
        match std::str::from_utf8(pending) {
            Ok(text) => {
                if !text.is_empty() {
                    on_chunk(stream, text);
                    forwarded += text.len();
                }
                pending.clear();
                return forwarded;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                if valid > 0 {
                    let text = std::str::from_utf8(&pending[..valid]).unwrap_or_default();
                    on_chunk(stream, text);
                    forwarded += valid;
                }
                let invalid = error.error_len();
                pending.drain(..valid);
                match invalid {
                    Some(len) => {
                        on_chunk(stream, "\u{FFFD}");
                        forwarded += "\u{FFFD}".len();
                        pending.drain(..len.min(pending.len()));
                    }
                    // The tail may be the first half of a multi-byte character;
                    // keep it until the next read completes it.
                    None => return forwarded,
                }
            }
        }
    }
}

/// Read at most `max` bytes, draining the rest so the writer never blocks.
pub fn read_capped<R: Read>(mut reader: R, max: usize) -> Vec<u8> {
    let mut kept = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                if kept.len() < max {
                    let take = (max - kept.len()).min(n);
                    kept.extend_from_slice(&chunk[..take]);
                }
            }
            Err(_) => break,
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn read_capped_bounds_and_drains_output() {
        let data = vec![b'x'; 100_000];
        let kept = read_capped(std::io::Cursor::new(data), 4_096);
        assert_eq!(kept.len(), 4_096);
    }

    #[cfg(unix)]
    #[test]
    fn a_hung_child_is_killed_at_the_deadline() {
        let mut child = Command::new("sleep")
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn sleep");
        let started = Instant::now();
        let captured = wait_captured(&mut child, Duration::from_millis(100), 4096).unwrap();
        assert!(captured.status.is_none(), "the child must have been killed");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "waiting must not outlive the deadline"
        );
    }

    #[cfg(unix)]
    #[test]
    fn normal_output_is_captured() {
        let mut child = Command::new("sh")
            .args(["-c", "printf hello; printf oops >&2"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn sh");
        let captured = wait_captured(&mut child, Duration::from_secs(5), 4096).unwrap();
        assert!(captured.status.is_some_and(|status| status.success()));
        assert_eq!(String::from_utf8_lossy(&captured.stdout), "hello");
        assert_eq!(String::from_utf8_lossy(&captured.stderr), "oops");
    }

    #[cfg(unix)]
    #[test]
    fn streaming_forwards_chunks_and_keeps_the_final_capture() {
        let mut child = Command::new("sh")
            .args(["-c", "printf one; sleep 0.05; printf two; printf err >&2"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn sh");
        let cancel = AtomicBool::new(false);
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = std::sync::Arc::clone(&seen);
        let streamed = wait_streaming(
            &mut child,
            Duration::from_secs(5),
            4096,
            &cancel,
            move |stream, text| {
                recorded.lock().unwrap().push((stream, text.to_string()));
            },
        )
        .unwrap();
        assert!(!streamed.cancelled);
        assert!(
            streamed
                .captured
                .status
                .is_some_and(|status| status.success())
        );
        assert_eq!(String::from_utf8_lossy(&streamed.captured.stdout), "onetwo");
        let seen = seen.lock().unwrap();
        let stdout: String = seen
            .iter()
            .filter(|(stream, _)| *stream == Stream::Stdout)
            .map(|(_, text)| text.as_str())
            .collect();
        assert_eq!(stdout, "onetwo");
    }

    #[cfg(unix)]
    #[test]
    fn a_cancelled_child_is_killed_promptly() {
        let mut child = Command::new("sleep")
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn sleep");
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let flagged = std::sync::Arc::clone(&cancel);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            flagged.store(true, Ordering::Relaxed);
        });
        let started = Instant::now();
        let streamed = wait_streaming(
            &mut child,
            Duration::from_secs(30),
            4096,
            &cancel,
            |_, _| {},
        )
        .unwrap();
        assert!(streamed.cancelled);
        assert!(streamed.captured.status.is_none());
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
