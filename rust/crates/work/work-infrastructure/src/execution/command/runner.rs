//! Execute one bound invocation in the inherited host environment; never install a policy.
use super::host_environment;
use crate::process::host;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use work_feature::error::{ExitCode, WorkError};
use work_feature::execution::ExecutionWriterContext;
use work_feature::ports::{CommandOutcome, CommandRequest, CommandRunner, CommandStatus};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static INTERRUPT_OWNER: Mutex<()> = Mutex::new(());

fn invalid() -> WorkError {
    WorkError::new(
        ExitCode::ArtifactIntegrity,
        "command_invocation_changed",
        "The bound invocation or source changed. Preserve receipts and review a new preview; never rerun automatically.",
        json!({}),
    )
}
pub(in crate::execution) struct HostCommandRunner<'a> {
    context: &'a ExecutionWriterContext,
    approved: RefCell<Option<(CommandRequest, Value)>>,
}
impl<'a> HostCommandRunner<'a> {
    pub(in crate::execution) fn for_task(context: &'a ExecutionWriterContext) -> Self {
        Self {
            context,
            approved: RefCell::new(None),
        }
    }
    pub(in crate::execution) fn bind(&self, preview: &Value) -> Result<(), WorkError> {
        let _: work_model::execution::response::PreparedCommandPreview =
            serde_json::from_value(preview.clone()).map_err(|_| invalid())?;
        let approval = preview["approved_sha256"].as_str().ok_or_else(invalid)?;
        work_feature::execution::command_publication::approved_receipt_paths(
            preview,
            self.context.target().execution_dir,
            approval,
        )?;
        if preview["task_id"] != self.context.target().task_id
            || preview["execution"]["file_version"]
                != json!(super::super::file_transaction::storage::current_version(
                    self.context,
                    preview["attempt_id"].as_str().ok_or_else(invalid)?
                )?)
            || preview["execution"]["host"] != host_environment()
            || preview["execution"]["input_files"] != super::input_files(self.context)?
        {
            return Err(invalid());
        }
        let request = work_feature::execution::command_publication::build_command_request(preview)?;
        *self.approved.borrow_mut() = Some((request, preview.clone()));
        Ok(())
    }
    fn check(&self, request: &CommandRequest) -> Result<Vec<File>, WorkError> {
        let (approved, preview) = self.approved.borrow_mut().take().ok_or_else(invalid)?;
        if preview["execution"]["input_files"] != super::input_files(self.context)?
            || preview["execution"]["file_version"]
                != json!(super::super::file_transaction::storage::current_version(
                    self.context,
                    preview["attempt_id"].as_str().ok_or_else(invalid)?
                )?)
        {
            return Err(invalid());
        }
        if request.argv != approved.argv
            || request.cwd != approved.cwd
            || request.timeout != approved.timeout
            || !request
                .cwd
                .canonicalize()
                .map_err(|_| invalid())?
                .starts_with(&self.context.writer().canonical_project_root)
        {
            return Err(invalid());
        }
        for (path, hash) in preview["sources"].as_object().ok_or_else(invalid)? {
            let path = crate::files::resolve_runtime_path(
                &self.context.writer().canonical_project_root,
                path,
            )?;
            let bytes = super::super::evidence_file::read(&path, 256 * 1024 * 1024)?;
            if *hash != work_operations::derivation::fingerprint::raw(&bytes) {
                return Err(invalid());
            }
        }
        let entries: &[(&str, &str)] = match preview["invocation"]["kind"].as_str() {
            Some("direct") => &[("executable", "executable_sha256")],
            Some("windows_batch") if cfg!(windows) => {
                &[("launcher", "launcher_sha256"), ("script", "script_sha256")]
            }
            _ => return Err(invalid()),
        };
        let mut held = Vec::new();
        for (path_key, hash_key) in entries {
            let path = Path::new(
                preview["invocation"][*path_key]
                    .as_str()
                    .ok_or_else(invalid)?,
            );
            if !path.is_absolute() {
                return Err(invalid());
            }
            let mut options = OpenOptions::new();
            options.read(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options.share_mode(1);
            }
            // Follow the reviewed executable name, including virtual-environment links.
            // No tool, dependency or executable copy is created.
            let file = options.open(path).map_err(|_| invalid())?;
            let before = file.metadata().map_err(|_| invalid())?;
            if !before.is_file() || before.len() > 256 * 1024 * 1024 {
                return Err(invalid());
            }
            let (length, hash) = work_operations::derivation::fingerprint::raw_reader(
                (&file).take(256 * 1024 * 1024 + 1),
            )
            .map_err(|_| invalid())?;
            let after = file.metadata().map_err(|_| invalid())?;
            if length != before.len()
                || before.len() != after.len()
                || before.modified().map_err(|_| invalid())?
                    != after.modified().map_err(|_| invalid())?
                || preview["invocation"][*hash_key] != hash
            {
                return Err(invalid());
            }
            held.push(file);
        }
        Ok(held)
    }
}
impl CommandRunner for HostCommandRunner<'_> {
    fn run(&self, request: &CommandRequest) -> CommandOutcome {
        let held = match self.check(request) {
            Ok(held) => held,
            Err(error) => return failed(error),
        };
        let guard = match InterruptGuard::install() {
            Ok(guard) => guard,
            Err(error) => {
                return failed(WorkError::new(
                    ExitCode::IoFailure,
                    "command_interrupt_unavailable",
                    "Interruption handling could not be installed; no command was launched.",
                    json!({"os_error":error.raw_os_error(),"error":error.to_string()}),
                ));
            }
        };
        let capture = host::run(request, &INTERRUPTED);
        let handler_cleanup = guard.finish().err();
        drop(held);
        host::into_outcome(capture, handler_cleanup)
    }
}
fn failed(error: WorkError) -> CommandOutcome {
    CommandOutcome {
        status: CommandStatus::LaunchFailed,
        exit_code: None,
        stdout_tail: String::new(),
        stderr_tail: String::new(),
        stdout_truncated: false,
        stderr_truncated: false,
        signal: None,
        complete_output: None,
        diagnostic: Some(json!({
            "reason_code":error.reason_code,"message":error.message,"details":error.details})),
        timing: None,
    }
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> i32>,
        add: i32,
    ) -> i32;
}
#[cfg(windows)]
unsafe extern "system" fn interrupt(event: u32) -> i32 {
    if event <= 1 {
        INTERRUPTED.store(true, Ordering::Release);
        1
    } else {
        0
    }
}
#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn signal(number: i32, handler: usize) -> usize;
}
#[cfg(target_os = "macos")]
extern "C" fn interrupt(_: i32) {
    INTERRUPTED.store(true, Ordering::Release);
}

struct InterruptGuard {
    _owner: MutexGuard<'static, ()>,
    #[cfg(windows)]
    installed: bool,
    #[cfg(target_os = "macos")]
    previous: Option<(usize, usize)>,
}
impl InterruptGuard {
    fn install() -> io::Result<Self> {
        let owner = INTERRUPT_OWNER
            .lock()
            .map_err(|_| io::Error::other("Interruption owner is poisoned"))?;
        INTERRUPTED.store(false, Ordering::Release);
        #[cfg(windows)]
        {
            let installed = unsafe { SetConsoleCtrlHandler(Some(interrupt), 1) } != 0;
            if !installed {
                let error = io::Error::last_os_error();
                // A detached process has no console events. Parent death still closes the lifetime Job.
                if error.raw_os_error() != Some(6) {
                    return Err(error);
                }
            }
            Ok(Self {
                _owner: owner,
                installed,
            })
        }
        #[cfg(target_os = "macos")]
        {
            let handler = interrupt as *const () as usize;
            let first = unsafe { signal(2, handler) };
            if first == usize::MAX {
                return Err(io::Error::last_os_error());
            }
            let second = unsafe { signal(15, handler) };
            if second == usize::MAX {
                let error = io::Error::last_os_error();
                unsafe {
                    signal(2, first);
                }
                return Err(error);
            }
            Ok(Self {
                _owner: owner,
                previous: Some((first, second)),
            })
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            drop(owner);
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Unsupported host platform",
            ))
        }
    }
    fn restore(&mut self) -> io::Result<()> {
        #[cfg(windows)]
        if self.installed {
            self.installed = false;
            if unsafe { SetConsoleCtrlHandler(Some(interrupt), 0) } == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        #[cfg(target_os = "macos")]
        if let Some((first, second)) = self.previous.take() {
            let result1 = unsafe { signal(2, first) };
            let result2 = unsafe { signal(15, second) };
            if result1 == usize::MAX || result2 == usize::MAX {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
    fn finish(mut self) -> io::Result<()> {
        self.restore()
    }
}
impl Drop for InterruptGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incomplete_or_unknown_execution_never_fabricates_an_exit_code() {
        let request = CommandRequest {
            argv: vec!["work-missing-tool-81".into()],
            cwd: std::env::temp_dir(),
            timeout: std::time::Duration::from_millis(50),
        };
        let actual = host::run(&request, &AtomicBool::new(false));
        let result = host::into_outcome(actual, None);
        assert_eq!(result.status, CommandStatus::LaunchFailed);
        assert_eq!(result.exit_code, None);
        assert!(result.diagnostic.is_some() && result.timing.is_some());
        let cancelled = host::run(&request, &AtomicBool::new(true));
        let result = host::into_outcome(cancelled, None);
        assert_eq!(result.status, CommandStatus::Interrupted);
        assert_eq!(result.exit_code, None);
        assert!(!result.complete_output.unwrap().stdout_complete);
    }
}
