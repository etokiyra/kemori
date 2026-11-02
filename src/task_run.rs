//! Running one project task with streamed, bounded output.
//!
//! A task is the project's own command. It runs without a shell, streams its
//! stdout/stderr through [`crate::process::wait_streaming`], keeps a per-stream
//! byte cap and is killed at a deadline or when the user cancels. The caller
//! owns delivery of the chunks, so this module stays independent of the
//! background event type.

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::process::Stream;
use crate::project::tasks::Task;

/// How long a run task may execute before the background worker kills it. A
/// blocked worker is acceptable (the UI stays responsive); an unbounded child
/// is not.
const TASK_TIMEOUT: Duration = Duration::from_secs(600);

/// The most output bytes a run task may leave behind, beyond which the tail is
/// drained and dropped.
const TASK_OUTPUT_CAP: usize = 256 * 1024;

/// How a run task ended and what it printed.
pub struct TaskRunOutcome {
    pub task: Task,
    /// Success when the process exited cleanly; `None` means killed or lost.
    pub exit_code: Option<i32>,
    /// True when the task ran past Kemori's deadline and was killed.
    pub timed_out: bool,
    /// True when the child could not be spawned at all.
    pub spawn_error: Option<String>,
    /// True when the user cancelled the task.
    pub cancelled: bool,
    /// stdout and stderr, each capped, stdout first.
    pub text: String,
    /// Whether the capped stream was truncated before the task ended.
    pub truncated: bool,
}

/// Run one task in `root`, forwarding live chunks and capturing the bounded
/// result.
///
/// The command is spawned exactly as named: no shell, so project-provided
/// arguments cannot be reinterpreted. stdout and stderr are streamed and
/// captured on their own threads by [`crate::process::wait_streaming`].
pub fn run<F>(root: &Path, task: Task, cancel: &AtomicBool, on_chunk: F) -> TaskRunOutcome
where
    F: Fn(Stream, &str) + Send + Sync + 'static,
{
    let mut outcome = TaskRunOutcome {
        task: task.clone(),
        exit_code: None,
        timed_out: false,
        spawn_error: None,
        cancelled: false,
        text: String::new(),
        truncated: false,
    };
    let mut child = match Command::new(&task.program)
        .args(&task.args)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            outcome.spawn_error = Some(format!(
                "`{}' could not be started: {err}",
                task.command_line()
            ));
            return outcome;
        }
    };
    let streamed = match crate::process::wait_streaming(
        &mut child,
        TASK_TIMEOUT,
        TASK_OUTPUT_CAP,
        cancel,
        on_chunk,
    ) {
        Ok(streamed) => streamed,
        Err(err) => {
            outcome.spawn_error = Some(err.to_string());
            return outcome;
        }
    };
    let captured = streamed.captured;
    outcome.cancelled = streamed.cancelled;
    let status = captured.status;
    outcome.exit_code = status.as_ref().and_then(|status| status.code());
    outcome.timed_out = status.is_none() && !outcome.cancelled;
    outcome.truncated =
        captured.stdout.len() >= TASK_OUTPUT_CAP || captured.stderr.len() >= TASK_OUTPUT_CAP;
    let stdout_text = String::from_utf8_lossy(&captured.stdout);
    let stderr_text = String::from_utf8_lossy(&captured.stderr);
    match (stdout_text.trim().is_empty(), stderr_text.trim().is_empty()) {
        (true, true) => {}
        (false, true) => outcome.text += stdout_text.trim_end(),
        (true, false) => outcome.text += stderr_text.trim_end(),
        // Kept apart: the capture threads raced, so interleaving is fiction.
        (false, false) => {
            outcome.text = format!("{stdout_text}\n── stderr ──\n{stderr_text}");
        }
    }
    outcome
}
