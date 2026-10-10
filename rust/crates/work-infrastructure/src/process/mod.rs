//! Bounded host-inherited child process execution.
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use work_feature::ports::CommandStatus;
#[cfg(test)]
use work_feature::ports::{CommandOutcome, CommandRequest, CommandRunner};

pub(crate) mod host;
const TAIL_BYTES: usize = 4096;

pub(crate) struct ProcessCapture {
    pub status: CommandStatus,
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stdout_complete: bool,
}
pub(crate) fn run_argv(argv: &[String], cwd: &Path, timeout: Duration) -> ProcessCapture {
    let capture = host::run(
        &work_feature::ports::CommandRequest {
            argv: argv.to_vec(),
            cwd: cwd.into(),
            timeout,
        },
        &AtomicBool::new(false),
    );
    let outcome = host::into_outcome(capture, None);
    let output = outcome
        .complete_output
        .expect("the host runner retains known output");
    ProcessCapture {
        status: outcome.status,
        exit_code: outcome.exit_code,
        stdout_complete: output.stdout_complete,
        stdout: output.stdout,
    }
}
#[derive(Debug, Default, Clone, Copy)]
#[cfg(test)]
pub(crate) struct LocalCommandRunner;
#[cfg(test)]
impl CommandRunner for LocalCommandRunner {
    fn run(&self, request: &CommandRequest) -> CommandOutcome {
        host::into_outcome(host::run(request, &AtomicBool::new(false)), None)
    }
}

/// Retain a complete bounded stream independently of the diagnostic tail.
#[derive(Default)]
pub(crate) struct FullStream {
    pub bytes: Vec<u8>,
    pub overflowed: bool,
    pub read_failed: bool,
    pub eof: bool,
    /// Kernel-verified owned producers have exited and the pipe has no buffered bytes.
    pub source_closed: bool,
}

impl FullStream {
    pub fn append(&mut self, bytes: &[u8]) {
        let remaining = work_model::execution::command::capture::MAX_STREAM_BYTES
            .saturating_sub(self.bytes.len());
        self.bytes
            .extend_from_slice(&bytes[..remaining.min(bytes.len())]);
        self.overflowed |= bytes.len() > remaining;
    }
    pub fn complete(&self) -> bool {
        (self.eof || self.source_closed) && !self.overflowed && !self.read_failed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[cfg(target_os = "macos")]
    use std::time::Instant;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn full_capture_never_marks_overflow_read_error_or_missing_eof_complete() {
        let mut stream = FullStream::default();
        stream.append(b"complete bytes");
        assert!(!stream.complete());
        stream.eof = true;
        assert!(stream.complete());
        stream.read_failed = true;
        assert!(!stream.complete());
        stream.read_failed = false;
        stream.append(&vec![1; work_model::execution::command::capture::MAX_STREAM_BYTES]);
        assert_eq!(
            stream.bytes.len(),
            work_model::execution::command::capture::MAX_STREAM_BYTES
        );
        assert!(!stream.complete());
    }

    fn root() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "work rust process {} {}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        root
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn argv_preserves_spaces_and_tail_capture_is_bounded() {
        let cwd = root();
        let run = |argv: Vec<String>| {
            LocalCommandRunner.run(&CommandRequest {
                argv,
                cwd: cwd.clone(),
                timeout: Duration::from_secs(3),
            })
        };
        let spaced = run(vec![
            "/usr/bin/printf".into(),
            "%s".into(),
            "a b 中文".into(),
        ]);
        assert_eq!(spaced.status, CommandStatus::Exited);
        assert_eq!(spaced.stdout_tail, "a b 中文");
        assert_eq!(spaced.exit_code, Some(0));
        let large = run(vec![
            "awk".into(),
            "BEGIN { for (i=0; i<5000; i++) printf \"x\"; for (i=0; i<5000; i++) printf \"y\" > \"/dev/stderr\" }".into(),
        ]);
        assert_eq!(large.status, CommandStatus::Exited);
        assert_eq!(large.stdout_tail.len(), TAIL_BYTES);
        assert!(large.stdout_truncated);
        assert_eq!(large.stderr_tail.len(), TAIL_BYTES);
        assert!(large.stderr_truncated);
        assert!(large.stdout_tail.bytes().all(|b| b == b'x'));
        let concurrent = run(vec![
            "awk".into(),
            "BEGIN { for (i=0; i<200000; i++) printf \"o\"; for (i=0; i<200000; i++) printf \"e\" > \"/dev/stderr\"; exit 7 }".into(),
        ]);
        assert_eq!(concurrent.status, CommandStatus::Exited);
        assert_eq!(concurrent.exit_code, Some(7));
        assert_eq!(concurrent.stdout_tail, "o".repeat(TAIL_BYTES));
        assert_eq!(concurrent.stderr_tail, "e".repeat(TAIL_BYTES));
        assert!(concurrent.stdout_truncated && concurrent.stderr_truncated);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn timeout_kills_child_and_launch_failure_is_reported() {
        let cwd = root();
        let started = Instant::now();
        let timed = LocalCommandRunner.run(&CommandRequest {
            argv: vec!["/bin/sleep".into(), "2".into()],
            cwd: cwd.clone(),
            timeout: Duration::from_millis(50),
        });
        assert_eq!(timed.status, CommandStatus::TimedOut);
        assert_eq!(timed.exit_code, None);
        assert!(started.elapsed() < Duration::from_secs(1));
        let with_output = LocalCommandRunner.run(&CommandRequest {
            argv: vec![
                "/bin/sh".into(),
                "-c".into(),
                "printf started; exec sleep 3".into(),
            ],
            cwd: cwd.clone(),
            timeout: Duration::from_millis(200),
        });
        assert_eq!(with_output.status, CommandStatus::TimedOut);
        assert_eq!(with_output.exit_code, None);
        assert!(with_output.stdout_tail.contains("started"));
        let missing = LocalCommandRunner.run(&CommandRequest {
            argv: vec!["work-command-that-does-not-exist".into()],
            cwd,
            timeout: Duration::from_secs(1),
        });
        assert_eq!(missing.status, CommandStatus::LaunchFailed);
        assert_eq!(missing.exit_code, None);
    }

    #[cfg(windows)]
    #[test]
    fn windows_argv_timeout_and_launch_failure() {
        let cwd = root();
        let spaced = LocalCommandRunner.run(&CommandRequest {
            argv: vec!["cmd.exe".into(), "/C".into(), "echo a b".into()],
            cwd: cwd.clone(),
            timeout: Duration::from_secs(3),
        });
        assert_eq!(spaced.status, CommandStatus::Exited);
        assert_eq!(spaced.stdout_tail.trim(), "a b");
        let timed = LocalCommandRunner.run(&CommandRequest {
            argv: vec![
                "powershell.exe".into(),
                "-NoProfile".into(),
                "-Command".into(),
                "Start-Sleep -Seconds 2".into(),
            ],
            cwd: cwd.clone(),
            timeout: Duration::from_millis(50),
        });
        assert_eq!(timed.status, CommandStatus::TimedOut);
        let missing = LocalCommandRunner.run(&CommandRequest {
            argv: vec!["work-command-that-does-not-exist".into()],
            cwd,
            timeout: Duration::from_secs(1),
        });
        assert_eq!(missing.status, CommandStatus::LaunchFailed);
    }
}
