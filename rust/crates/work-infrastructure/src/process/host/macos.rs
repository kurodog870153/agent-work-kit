//! Darwin process groups inherit the host's token, environment and restrictions.
use super::PollRead;
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use work_feature::ports::CommandRequest;

unsafe extern "C" {
    fn fcntl(fd: i32, command: i32, ...) -> i32;
    fn kill(pid: i32, signal: i32) -> i32;
    fn poll(fds: *mut PollFd, count: u32, timeout: i32) -> i32;
}
#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

fn wait_readable(stdout: i32, stderr: i32, timeout: i32) -> io::Result<bool> {
    let mut fds = [stdout, stderr].map(|fd| PollFd {
        fd,
        events: 0x0001, // Darwin POLLIN; negative descriptors are ignored.
        revents: 0,
    });
    // SAFETY: Both descriptors are borrowed from live pipes; the array has two entries.
    let result = unsafe { poll(fds.as_mut_ptr(), 2, timeout) };
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(result > 0)
    }
}
pub(super) struct Pipe<T>(T);
impl<T: Read + AsRawFd> Pipe<T> {
    fn new(pipe: T) -> io::Result<Self> {
        // Darwin F_GETFL/F_SETFL and O_NONBLOCK, applied only to our owned read end.
        let flags = unsafe { fcntl(pipe.as_raw_fd(), 3) };
        if flags < 0 || unsafe { fcntl(pipe.as_raw_fd(), 4, flags | 0x4) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(pipe))
    }
}
impl<T: Read> PollRead for Pipe<T> {
    fn poll_read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.0.read(bytes)
    }
}
pub(super) struct Child {
    child: std::process::Child,
    pub stdout: Pipe<std::process::ChildStdout>,
    pub stderr: Pipe<std::process::ChildStderr>,
    terminated: bool,
}
impl Child {
    pub fn wait_for_output(&self, stdout: bool, stderr: bool) {
        // Wake as soon as either producer writes, rather than delaying every pipe refill.
        // Keep the wait bounded so cancellation and process deadlines are still checked.
        if let Err(error) = wait_readable(
            if stdout {
                self.stdout.0.as_raw_fd()
            } else {
                -1
            },
            if stderr {
                self.stderr.0.as_raw_fd()
            } else {
                -1
            },
            5,
        ) && error.kind() != io::ErrorKind::Interrupted
        {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn spawn(request: &CommandRequest) -> io::Result<Self> {
        let mut child = Command::new(&request.argv[0])
            .args(&request.argv[1..])
            .current_dir(&request.cwd)
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let streams = Pipe::new(child.stdout.take().expect("piped stdout")).and_then(|out| {
            Pipe::new(child.stderr.take().expect("piped stderr")).map(|err| (out, err))
        });
        match streams {
            Ok((stdout, stderr)) => Ok(Self {
                child,
                stdout,
                stderr,
                terminated: false,
            }),
            Err(error) => {
                unsafe {
                    kill(-(child.id() as i32), 9);
                }
                let _ = child.wait();
                Err(error)
            }
        }
    }
    pub fn poll(&mut self) -> io::Result<Option<(Option<i32>, Option<i32>)>> {
        self.child
            .try_wait()
            .map(|exit| exit.map(|e| (e.code(), e.signal())))
    }
    pub fn terminate(&mut self) -> io::Result<()> {
        if self.terminated {
            return Ok(());
        }
        let result = unsafe { kill(-(self.child.id() as i32), 9) };
        if result != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(3) {
                return Err(error);
            } // ESRCH: no group remains.
        }
        let started = Instant::now();
        while self.child.try_wait()?.is_none() {
            if started.elapsed() >= Duration::from_millis(500) {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Process termination unverified",
                ));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        self.terminated = true;
        Ok(())
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::host::{Status, run};
    use std::sync::atomic::AtomicBool;
    #[test]
    fn pipe_wait_wakes_for_either_stream_and_ignores_closed_streams() {
        use std::io::Write;
        for selected in [0, 1] {
            // Stdio pipes are close-on-exec. Raw pipe() descriptors can leak into
            // children spawned by parallel tests and keep the write end alive.
            let mut producer = Command::new("/bin/sh")
                .args(["-c", "read -r start; printf ready; read -r finish; exit 0"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let mut reader = producer.stdout.take().unwrap();
            let mut writer = producer.stdin.take().unwrap();
            let mut fds = [-1; 2];
            fds[selected] = reader.as_raw_fd();
            // SAFETY: F_GETFD only inspects our live pipe descriptor.
            let flags = unsafe { fcntl(reader.as_raw_fd(), 1) };
            assert!(flags >= 0, "F_GETFD failed: {}", io::Error::last_os_error());
            assert_ne!(flags & 1, 0, "pipe must be close-on-exec");
            assert!(!wait_readable(fds[0], fds[1], 0).unwrap());
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    std::thread::sleep(Duration::from_millis(20));
                    writer.write_all(b"start\n").unwrap();
                });
                assert!(wait_readable(fds[0], fds[1], 1000).unwrap());
            });
            let mut bytes = [0; 5];
            reader.read_exact(&mut bytes).unwrap();
            assert_eq!(&bytes, b"ready");
            drop(writer);
            assert!(producer.wait().unwrap().success());
            assert!(
                wait_readable(fds[0], fds[1], 0).unwrap(),
                "EOF wakes the reader"
            );
            assert_eq!(reader.read(&mut bytes).unwrap(), 0);
        }
        assert!(!wait_readable(-1, -1, 0).unwrap());
    }

    #[test]
    fn actual_signal_preserves_output_without_an_invented_numeric_exit() {
        let capture = run(
            &work_feature::ports::CommandRequest {
                argv: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    "printf actual-signal; kill -TERM $$".into(),
                ],
                cwd: std::env::temp_dir(),
                timeout: std::time::Duration::from_secs(3),
            },
            &AtomicBool::new(false),
        );
        assert_eq!(capture.status, Status::Exited);
        assert_eq!(capture.exit_code, None);
        assert_eq!(capture.signal, Some(15));
        assert_eq!(capture.stdout.full.bytes, b"actual-signal");
        assert!(capture.stdout.full.complete() && capture.stderr.full.complete());
    }
}
