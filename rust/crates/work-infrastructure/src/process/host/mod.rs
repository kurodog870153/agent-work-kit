//! Private host-inherited execution. Jobs/groups manage lifetime only, never permissions.
use super::{FullStream, TAIL_BYTES};
use serde_json::json;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};
use work_feature::ports::{CommandOutcome, CommandOutputCapture, CommandRequest, CommandStatus};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(windows)]
use windows as platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    Exited,
    TimedOut,
    Interrupted,
    LaunchFailed,
    Unknown,
}

#[derive(Default)]
pub(crate) struct Stream {
    pub full: FullStream,
    pub tail: Vec<u8>,
    pub truncated: bool,
}
impl Stream {
    fn append(&mut self, bytes: &[u8]) {
        self.full.append(bytes);
        self.tail.extend_from_slice(bytes);
        if self.tail.len() > TAIL_BYTES {
            self.truncated = true;
            self.tail.drain(..self.tail.len() - TAIL_BYTES);
        }
    }
    fn drain(&mut self, pipe: &mut impl PollRead) -> bool {
        let mut progressed = false;
        // Bound work per tick, including a command that continuously floods stdout.
        for _ in 0..16 {
            if self.full.eof || self.full.source_closed || self.full.read_failed {
                return progressed;
            }
            let mut bytes = [0; 16384];
            match pipe.poll_read(&mut bytes) {
                Ok(0) => self.full.eof = true,
                Ok(n) => {
                    self.append(&bytes[..n]);
                    progressed = true;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    self.full.source_closed = pipe.source_closed();
                    return progressed;
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => self.full.read_failed = true,
            }
        }
        progressed
    }
}
pub(super) trait PollRead {
    fn poll_read(&mut self, bytes: &mut [u8]) -> io::Result<usize>;
    fn source_closed(&self) -> bool {
        false
    }
}
pub(crate) struct Capture {
    pub started_at: SystemTime,
    pub finished_at: SystemTime,
    pub elapsed: Duration,
    pub status: Status,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub stdout: Stream,
    pub stderr: Stream,
    pub error: Option<io::Error>,
    pub cleanup_error: Option<io::Error>,
}
impl Capture {
    fn new(status: Status) -> Self {
        Self {
            started_at: SystemTime::now(),
            finished_at: SystemTime::now(),
            elapsed: Duration::ZERO,
            status,
            exit_code: None,
            signal: None,
            stdout: Stream::default(),
            stderr: Stream::default(),
            error: None,
            cleanup_error: None,
        }
    }
}

pub(crate) fn run(request: &CommandRequest, cancelled: &AtomicBool) -> Capture {
    let started_at = SystemTime::now();
    let started = Instant::now();
    let mut capture = run_inner(request, cancelled);
    capture.started_at = started_at;
    capture.finished_at = SystemTime::now();
    capture.elapsed = started.elapsed();
    capture
}

fn run_inner(request: &CommandRequest, cancelled: &AtomicBool) -> Capture {
    if cancelled.load(Ordering::Acquire) {
        return Capture::new(Status::Interrupted);
    }
    if request.argv.is_empty() || request.argv.iter().any(|s| s.contains('\0')) {
        let mut capture = Capture::new(Status::LaunchFailed);
        capture.error = Some(io::Error::new(io::ErrorKind::InvalidInput, "Invalid argv"));
        return capture;
    }
    #[cfg(any(windows, target_os = "macos"))]
    {
        run_supported(request, cancelled)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let mut capture = Capture::new(Status::LaunchFailed);
        capture.error = Some(io::Error::new(
            io::ErrorKind::Unsupported,
            "Unsupported host platform",
        ));
        capture
    }
}

#[cfg(any(windows, target_os = "macos"))]
fn run_supported(request: &CommandRequest, cancelled: &AtomicBool) -> Capture {
    let mut child = match platform::Child::spawn(request) {
        Ok(child) => child,
        Err(error) => {
            let mut capture = Capture::new(Status::LaunchFailed);
            capture.error = Some(error);
            return capture;
        }
    };
    let started = Instant::now();
    let mut capture = Capture::new(Status::Unknown);
    loop {
        let stdout_progressed = capture.stdout.drain(&mut child.stdout);
        let stderr_progressed = capture.stderr.drain(&mut child.stderr);
        match child.poll() {
            Ok(Some((exit_code, signal))) => {
                capture.status = Status::Exited;
                capture.exit_code = exit_code;
                capture.signal = signal;
                break;
            }
            Err(error) => {
                capture.error = Some(error);
                break;
            }
            Ok(None) => {}
        }
        if cancelled.load(Ordering::Acquire) {
            capture.status = Status::Interrupted;
            break;
        }
        if started.elapsed() >= request.timeout {
            capture.status = Status::TimedOut;
            break;
        }
        // Do not throttle an active producer, even after the capture limit is reached.
        // Each bounded tick still checks process exit, cancellation and timeout.
        if !stdout_progressed && !stderr_progressed {
            wait_for_output(&child, &capture);
        }
    }
    // Stop descendants even after the leader exits, before bounded pipe draining.
    if let Err(error) = child.terminate() {
        capture.cleanup_error = Some(error);
    }
    let draining = Instant::now();
    loop {
        let stdout_progressed = capture.stdout.drain(&mut child.stdout);
        let stderr_progressed = capture.stderr.drain(&mut child.stderr);
        if ((capture.stdout.full.eof
            || capture.stdout.full.source_closed
            || capture.stdout.full.read_failed)
            && (capture.stderr.full.eof
                || capture.stderr.full.source_closed
                || capture.stderr.full.read_failed))
            || draining.elapsed() >= Duration::from_millis(500)
        {
            break;
        }
        if !stdout_progressed && !stderr_progressed {
            wait_for_output(&child, &capture);
        }
    }
    capture
}

#[cfg(any(windows, target_os = "macos"))]
fn wait_for_output(child: &platform::Child, capture: &Capture) {
    #[cfg(target_os = "macos")]
    child.wait_for_output(
        !(capture.stdout.full.eof || capture.stdout.full.read_failed),
        !(capture.stderr.full.eof || capture.stderr.full.read_failed),
    );
    #[cfg(windows)]
    {
        let _ = (child, capture);
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub(crate) fn into_outcome(capture: Capture, handler_cleanup: Option<io::Error>) -> CommandOutcome {
    let status = if capture.cleanup_error.is_some() || handler_cleanup.is_some() {
        CommandStatus::Unknown
    } else {
        match capture.status {
            Status::Exited => CommandStatus::Exited,
            Status::TimedOut => CommandStatus::TimedOut,
            Status::Interrupted => CommandStatus::Interrupted,
            Status::LaunchFailed => CommandStatus::LaunchFailed,
            Status::Unknown => CommandStatus::Unknown,
        }
    };
    let diagnostic = if capture.error.is_some()
        || capture.cleanup_error.is_some()
        || handler_cleanup.is_some()
    {
        let describe = |error: &io::Error| json!({"os_error":error.raw_os_error(),"message":error.to_string()});
        Some(json!({"process":capture.error.as_ref().map(describe),
            "termination":capture.cleanup_error.as_ref().map(describe),
            "interrupt_handler":handler_cleanup.as_ref().map(describe)}))
    } else {
        None
    };
    let timing = work_model::execution::response::CommandTiming {
        started_at: chrono::DateTime::<chrono::Utc>::from(capture.started_at)
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        finished_at: chrono::DateTime::<chrono::Utc>::from(capture.finished_at)
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        elapsed_ms: capture.elapsed.as_millis().min(u64::MAX as u128) as u64,
    };
    let stdout_complete = capture.stdout.full.complete();
    let stderr_complete = capture.stderr.full.complete();
    CommandOutcome {
        status,
        exit_code: capture.exit_code,
        signal: capture.signal,
        stdout_tail: String::from_utf8_lossy(&capture.stdout.tail).into_owned(),
        stdout_truncated: capture.stdout.truncated,
        stderr_tail: String::from_utf8_lossy(&capture.stderr.tail).into_owned(),
        stderr_truncated: capture.stderr.truncated,
        complete_output: Some(CommandOutputCapture {
            stdout: capture.stdout.full.bytes,
            stderr: capture.stderr.full.bytes,
            stdout_complete,
            stderr_complete,
        }),
        diagnostic,
        timing: Some(timing),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicU64;
    static ID: AtomicU64 = AtomicU64::new(0);

    pub(crate) fn fixture(config: serde_json::Value) -> CommandRequest {
        named_fixture("work host 中文", config)
    }

    fn named_fixture(prefix: &str, config: serde_json::Value) -> CommandRequest {
        let root = std::env::temp_dir().join(format!(
            "{prefix} {} {}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("host-fixture.json"),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
        CommandRequest {
            argv: vec![
                std::env::current_exe()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                "--exact".into(),
                "process::host::tests::child_fixture".into(),
                "--nocapture".into(),
            ],
            cwd: root,
            timeout: Duration::from_secs(5),
        }
    }

    #[test]
    fn child_fixture() {
        let Ok(bytes) = fs::read("host-fixture.json") else {
            return;
        };
        let config: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        match config["mode"].as_str().unwrap() {
            "argv" => println!(
                "HOST_DATA:{}",
                serde_json::json!({
                "argv":std::env::args().collect::<Vec<_>>(), "cwd":std::env::current_dir().unwrap()})
            ),
            "output" => {
                use std::io::Write;
                let count = config["count"].as_u64().unwrap() as usize;
                std::io::stdout().write_all(&vec![b'o'; count]).unwrap();
                std::io::stderr().write_all(&vec![b'e'; count]).unwrap();
                std::process::exit(7);
            }
            "sleep" => {
                println!("started");
                fs::write("ready", b"ready").unwrap();
                std::thread::sleep(Duration::from_secs(2));
                fs::write("late-write", b"must not happen after termination").unwrap();
            }
            "descendant" => {
                let dir = PathBuf::from(config["child_dir"].as_str().unwrap());
                let _child = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "process::host::tests::child_fixture",
                        "--nocapture",
                    ])
                    .current_dir(&dir)
                    .stdin(std::process::Stdio::null())
                    .spawn()
                    .unwrap();
                let started = Instant::now();
                while !dir.join("ready").exists() && started.elapsed() < Duration::from_secs(3) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                assert!(dir.join("ready").exists());
                println!("descendant started");
                if config["wait"] == true {
                    std::thread::sleep(Duration::from_secs(4));
                }
                std::process::exit(0);
            }
            _ => panic!("unknown fixture"),
        }
    }
    #[test]
    fn bounded_stream_preserves_errors_partial_bytes_and_overflow() {
        struct Fake {
            stage: usize,
        }
        impl PollRead for Fake {
            fn poll_read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                self.stage += 1;
                match self.stage {
                    1 => Err(io::ErrorKind::Interrupted.into()),
                    2 => {
                        bytes[..3].copy_from_slice(b"a\0b");
                        Ok(3)
                    }
                    _ => Err(io::ErrorKind::PermissionDenied.into()),
                }
            }
        }
        let mut stream = Stream::default();
        stream.drain(&mut Fake { stage: 0 });
        assert_eq!(stream.full.bytes, b"a\0b");
        assert!(stream.full.read_failed);
        assert!(!stream.full.complete());
        stream.append(&vec![
            b'x';
            work_model::execution::command::capture::MAX_STREAM_BYTES
                + 1
        ]);
        assert!(stream.full.overflowed);
        assert_eq!(stream.tail, vec![b'x'; TAIL_BYTES]);
    }

    #[test]
    fn draining_reports_progress_after_overflow_and_yields_between_bounded_ticks() {
        struct Producer(usize);
        impl PollRead for Producer {
            fn poll_read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                if self.0 == 0 {
                    return Err(io::ErrorKind::WouldBlock.into());
                }
                self.0 -= 1;
                bytes[0] = b'x';
                Ok(1)
            }
        }
        let mut stream = Stream::default();
        let limit = work_model::execution::command::capture::MAX_STREAM_BYTES;
        stream.append(&vec![b'o'; limit]);
        let mut producer = Producer(17);
        assert!(stream.drain(&mut producer));
        assert_eq!(producer.0, 1, "A busy pipe must leave the tick bounded");
        assert!(stream.full.overflowed);
        assert_eq!(stream.full.bytes.len(), limit);
        assert!(stream.drain(&mut producer), "Progress survives WouldBlock");
        assert!(
            !stream.drain(&mut producer),
            "An idle pipe permits sleeping"
        );
        assert!(!stream.full.complete());
        assert!(stream.tail.ends_with(&[b'x'; 17]));
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn host_argv_cwd_and_real_nonzero_output_are_preserved() {
        let before = SystemTime::now();
        let elapsed = Instant::now();
        let mut request = fixture(serde_json::json!({"mode":"argv"}));
        let special = ["a b 中文", "", r#"quote"slash\tail\"#, "x&y|z"];
        for arg in special {
            request.argv.extend(["--skip".into(), arg.into()]);
        }
        let capture = run(&request, &AtomicBool::new(false));
        assert_eq!(capture.status, Status::Exited);
        assert_eq!(capture.exit_code, Some(0));
        assert!(capture.started_at >= before);
        assert!(capture.finished_at <= SystemTime::now());
        assert!(capture.elapsed > Duration::ZERO && capture.elapsed <= elapsed.elapsed());
        assert!(capture.cleanup_error.is_none());
        assert!(capture.stdout.full.complete() && capture.stderr.full.complete());
        let output = String::from_utf8(capture.stdout.full.bytes).unwrap();
        let start = output.find("HOST_DATA:").unwrap() + "HOST_DATA:".len();
        let payload: serde_json::Value =
            serde_json::from_str(output[start..].lines().next().unwrap()).unwrap();
        let argv = payload["argv"].as_array().unwrap();
        for arg in special {
            assert!(argv.iter().any(|a| a == arg), "{arg:?}");
        }
        assert_eq!(
            PathBuf::from(payload["cwd"].as_str().unwrap())
                .canonicalize()
                .unwrap(),
            request.cwd.canonicalize().unwrap()
        );

        let request = fixture(serde_json::json!({"mode":"output","count":200000}));
        let capture = run(&request, &AtomicBool::new(false));
        assert_eq!(capture.status, Status::Exited);
        assert_eq!(capture.exit_code, Some(7));
        assert!(capture.stdout.full.bytes.ends_with(&vec![b'o'; 200000]));
        assert_eq!(capture.stderr.full.bytes.len(), 200000);
        assert!(capture.stdout.full.complete() && capture.stderr.full.complete());
        assert!(capture.stdout.truncated && capture.stderr.truncated);
        assert_eq!(capture.stdout.tail, vec![b'o'; TAIL_BYTES]);
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn real_output_overflow_is_drained_without_claiming_complete_evidence() {
        let limit = work_model::execution::command::capture::MAX_STREAM_BYTES;
        let request = fixture(serde_json::json!({"mode":"output","count":limit + 65536}));
        let capture = run(&request, &AtomicBool::new(false));
        assert_eq!(capture.status, Status::Exited);
        assert_eq!(capture.exit_code, Some(7));
        assert!(capture.cleanup_error.is_none());
        for stream in [capture.stdout, capture.stderr] {
            assert_eq!(stream.full.bytes.len(), limit);
            assert!(stream.full.eof && stream.full.overflowed && stream.truncated);
            assert!(!stream.full.complete());
            assert_eq!(stream.tail.len(), TAIL_BYTES);
        }
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn timeout_cancellation_and_leader_exit_stop_descendants_without_pipe_join() {
        for wait in [false, true] {
            let descendant = fixture(serde_json::json!({"mode":"sleep"}));
            let mut request = fixture(
                serde_json::json!({"mode":"descendant","child_dir":descendant.cwd,"wait":wait}),
            );
            if wait {
                request.timeout = Duration::from_millis(700);
            }
            let started = Instant::now();
            let capture = run(&request, &AtomicBool::new(false));
            assert_eq!(
                capture.status,
                if wait {
                    Status::TimedOut
                } else {
                    Status::Exited
                }
            );
            assert!(capture.cleanup_error.is_none());
            assert!(started.elapsed() < Duration::from_secs(3));
            assert!(capture.stdout.full.complete() && capture.stderr.full.complete());
            if wait {
                assert_eq!(capture.exit_code, None);
            }
            std::thread::sleep(Duration::from_millis(50));
            assert!(!descendant.cwd.join("late-write").exists());
        }
        let request = fixture(serde_json::json!({"mode":"sleep"}));
        let cancelled = AtomicBool::new(false);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let started = Instant::now();
                while !request.cwd.join("ready").exists()
                    && started.elapsed() < Duration::from_secs(3)
                {
                    std::thread::sleep(Duration::from_millis(5));
                }
                cancelled.store(true, Ordering::Release);
            });
            let capture = run(&request, &cancelled);
            assert_eq!(capture.status, Status::Interrupted);
            assert_eq!(capture.exit_code, None);
            assert!(capture.cleanup_error.is_none());
            assert!(String::from_utf8_lossy(&capture.stdout.full.bytes).contains("started"));
        });
    }
    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn missing_program_and_prelaunch_cancel_never_invent_exit_status() {
        let mut request = fixture(serde_json::json!({"mode":"argv"}));
        request.argv = vec!["work-missing-command-issue-81".into()];
        let capture = run(&request, &AtomicBool::new(false));
        assert_eq!(capture.status, Status::LaunchFailed);
        assert!(capture.error.is_some());
        assert_eq!(capture.exit_code, None);
        let capture = run(&request, &AtomicBool::new(true));
        assert_eq!(capture.status, Status::Interrupted);
        assert!(!capture.stdout.full.complete());
    }
    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn ordinary_cargo_uses_current_files_and_normal_output() {
        // MinGW's linker on Windows CI cannot encode Chinese output paths. The
        // host Unicode argv/cwd contract is exercised by the native fixture above.
        let prefix = if cfg!(all(windows, target_env = "gnu")) {
            "work host cargo"
        } else {
            "work host 中文"
        };
        let mut request = named_fixture(prefix, serde_json::json!({"mode":"argv"}));
        request.timeout = Duration::from_secs(60);
        let root = &request.cwd;
        fs::write(root.join("uncommitted.txt"), b"current uncommitted bytes").unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"hostfixture\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
        )
        .unwrap();
        fs::write(
            root.join("src/main.rs"),
            "fn main() { print!(\"{}\",include_str!(\"../uncommitted.txt\")); }\n",
        )
        .unwrap();
        let target = if cfg!(windows) {
            format!(
                "{}-pc-windows-{}",
                std::env::consts::ARCH,
                if cfg!(target_env = "gnu") {
                    "gnu"
                } else {
                    "msvc"
                }
            )
        } else {
            format!("{}-apple-darwin", std::env::consts::ARCH)
        };
        request.argv = vec![
            "cargo".into(),
            "build".into(),
            "--offline".into(),
            "--target".into(),
            target.clone(),
        ];
        let build = run(&request, &AtomicBool::new(false));
        assert_eq!(build.status, Status::Exited, "{:?}", build.error);
        assert_eq!(
            build.exit_code,
            Some(0),
            "{}",
            String::from_utf8_lossy(&build.stderr.full.bytes)
        );
        assert!(build.stdout.full.complete() && build.stderr.full.complete());
        let binary = root
            .join("target")
            .join(target)
            .join("debug")
            .join(if cfg!(windows) {
                "hostfixture.exe"
            } else {
                "hostfixture"
            });
        assert!(binary.is_file() && root.join("Cargo.lock").is_file());
        request.argv = vec![binary.to_str().unwrap().into()];
        let binary = run(&request, &AtomicBool::new(false));
        assert_eq!(binary.exit_code, Some(0));
        assert_eq!(binary.stdout.full.bytes, b"current uncommitted bytes");
        assert_eq!(
            fs::read(root.join("uncommitted.txt")).unwrap(),
            b"current uncommitted bytes"
        );
        assert!(root.join("target").is_dir());
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn ordinary_maven_uses_current_files_and_normal_output() {
        let mut request = fixture(serde_json::json!({"mode":"argv"}));
        request.timeout = Duration::from_secs(60);
        let root = &request.cwd;
        fs::write(root.join("uncommitted.txt"), b"current uncommitted bytes").unwrap();
        fs::write(root.join("pom.xml"),
            "<project><modelVersion>4.0.0</modelVersion><groupId>test</groupId><artifactId>hostfixture</artifactId><version>1</version></project>").unwrap();
        request.argv = vec!["mvn".into(), "-o".into(), "-q".into(), "validate".into()];
        let maven = run(&request, &AtomicBool::new(false));
        assert_eq!(maven.status, Status::Exited, "{:?}", maven.error);
        assert_eq!(
            maven.exit_code,
            Some(0),
            "{}",
            String::from_utf8_lossy(&maven.stderr.full.bytes)
        );
        assert!(maven.stdout.full.complete() && maven.stderr.full.complete());
        assert_eq!(
            fs::read(root.join("uncommitted.txt")).unwrap(),
            b"current uncommitted bytes"
        );
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn ordinary_npm_uses_current_files_and_normal_output() {
        let mut request = fixture(serde_json::json!({"mode":"argv"}));
        request.timeout = Duration::from_secs(60);
        let root = &request.cwd;
        fs::write(root.join("uncommitted.txt"), b"current uncommitted bytes").unwrap();
        fs::write(root.join("package.json"),serde_json::to_vec(&serde_json::json!({
            "name":"hostfixture","version":"1.0.0","private":true,"scripts":{"check":"node check.js"}})).unwrap()).unwrap();
        fs::write(root.join("check.js"),
            "const fs=require('fs');fs.mkdirSync('tool-output',{recursive:true});fs.writeFileSync('tool-output/npm.txt',fs.readFileSync('uncommitted.txt'));console.log('actual npm');").unwrap();
        request.argv = vec![
            "npm".into(),
            "--offline".into(),
            "run".into(),
            "check".into(),
        ];
        let npm = run(&request, &AtomicBool::new(false));
        assert_eq!(npm.status, Status::Exited, "{:?}", npm.error);
        assert_eq!(
            npm.exit_code,
            Some(0),
            "{}",
            String::from_utf8_lossy(&npm.stderr.full.bytes)
        );
        assert!(npm.stdout.full.complete() && npm.stderr.full.complete());
        assert_eq!(
            fs::read(root.join("tool-output/npm.txt")).unwrap(),
            b"current uncommitted bytes"
        );
        assert_eq!(
            fs::read(root.join("uncommitted.txt")).unwrap(),
            b"current uncommitted bytes"
        );
    }
}
