//! Windows Job containment controls lifetime; token/environment remain host-owned.
use super::PollRead;
use std::ffi::c_void;
use std::fs::File;
use std::io::{self, Read};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use work_feature::ports::CommandRequest;

// ABI layouts and ownership were extracted from the existing native launcher.
type RawHandle = *mut c_void;
#[repr(C)]
struct SecurityAttributes {
    length: u32,
    descriptor: *mut c_void,
    inherit: i32,
}
#[repr(C)]
struct Startup {
    size: u32,
    reserved: *mut u16,
    desktop: *mut u16,
    title: *mut u16,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    chars_x: u32,
    chars_y: u32,
    fill: u32,
    flags: u32,
    show: u16,
    reserved_bytes: u16,
    reserved_pointer: *mut u8,
    input: RawHandle,
    output: RawHandle,
    error: RawHandle,
}
#[repr(C)]
struct StartupEx {
    startup: Startup,
    attributes: *mut c_void,
}
#[repr(C)]
struct ProcessInformation {
    process: RawHandle,
    thread: RawHandle,
    process_id: u32,
    thread_id: u32,
}
#[repr(C)]
struct BasicLimits {
    process_time: i64,
    job_time: i64,
    flags: u32,
    minimum: usize,
    maximum: usize,
    active: u32,
    affinity: usize,
    priority: u32,
    scheduling: u32,
}
#[repr(C)]
struct ExtendedLimits {
    basic: BasicLimits,
    io: [u64; 6],
    process_memory: usize,
    job_memory: usize,
    peak_process_memory: usize,
    peak_job_memory: usize,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CloseHandle(handle: RawHandle) -> i32;
    fn CreatePipe(
        read: *mut RawHandle,
        write: *mut RawHandle,
        attributes: *mut SecurityAttributes,
        size: u32,
    ) -> i32;
    fn SetHandleInformation(handle: RawHandle, mask: u32, flags: u32) -> i32;
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        attributes: *mut SecurityAttributes,
        creation: u32,
        flags: u32,
        template: RawHandle,
    ) -> RawHandle;
    fn InitializeProcThreadAttributeList(
        list: *mut c_void,
        count: u32,
        flags: u32,
        size: *mut usize,
    ) -> i32;
    fn UpdateProcThreadAttribute(
        list: *mut c_void,
        flags: u32,
        attribute: usize,
        value: *mut c_void,
        size: usize,
        previous: *mut c_void,
        returned: *mut usize,
    ) -> i32;
    fn DeleteProcThreadAttributeList(list: *mut c_void);
    fn CreateProcessW(
        application: *const u16,
        command: *mut u16,
        process_attributes: *mut SecurityAttributes,
        thread_attributes: *mut SecurityAttributes,
        inherit: i32,
        flags: u32,
        environment: *mut c_void,
        cwd: *const u16,
        startup: *mut Startup,
        information: *mut ProcessInformation,
    ) -> i32;
    fn CreateJobObjectW(attributes: *mut SecurityAttributes, name: *const u16) -> RawHandle;
    fn SetInformationJobObject(
        job: RawHandle,
        class: i32,
        information: *mut c_void,
        length: u32,
    ) -> i32;
    fn TerminateJobObject(job: RawHandle, code: u32) -> i32;
    fn QueryInformationJobObject(
        job: RawHandle,
        class: i32,
        information: *mut c_void,
        size: u32,
        returned: *mut u32,
    ) -> i32;
    #[cfg(test)]
    fn GetCurrentProcess() -> RawHandle;
    #[cfg(test)]
    fn DuplicateHandle(
        source_process: RawHandle,
        source: RawHandle,
        target_process: RawHandle,
        target: *mut RawHandle,
        access: u32,
        inherit: i32,
        options: u32,
    ) -> i32;
    fn WaitForSingleObject(handle: RawHandle, timeout: u32) -> u32;
    fn GetExitCodeProcess(process: RawHandle, code: *mut u32) -> i32;
    fn PeekNamedPipe(
        pipe: RawHandle,
        buffer: *mut c_void,
        size: u32,
        read: *mut u32,
        available: *mut u32,
        remaining: *mut u32,
    ) -> i32;
}
#[repr(C)]
struct Accounting {
    times: [i64; 4],
    page_faults: u32,
    total: u32,
    active: u32,
    terminated: u32,
}
struct Handle(RawHandle);
impl Handle {
    fn new(raw: RawHandle) -> io::Result<Self> {
        if raw.is_null() || raw as isize == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(raw))
        }
    }
    fn file(self) -> File {
        let owned = std::mem::ManuallyDrop::new(self);
        // SAFETY: unique owned pipe handle transferred once.
        unsafe { File::from_raw_handle(owned.0) }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct Attributes {
    storage: Vec<u128>,
}
impl Attributes {
    fn new() -> io::Result<Self> {
        let mut size = 0;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut size);
        }
        if size == 0 || size > 65536 {
            return Err(io::Error::last_os_error());
        }
        let mut storage = vec![0u128; size.div_ceil(16)];
        check(unsafe {
            InitializeProcThreadAttributeList(storage.as_mut_ptr().cast(), 2, 0, &mut size)
        })?;
        Ok(Self { storage })
    }
    fn pointer(&mut self) -> *mut c_void {
        self.storage.as_mut_ptr().cast()
    }
    fn set<T>(&mut self, attribute: usize, value: &mut T) -> io::Result<()> {
        // SAFETY: caller keeps each payload alive through CreateProcessW.
        check(unsafe {
            UpdateProcThreadAttribute(
                self.pointer(),
                0,
                attribute,
                (value as *mut T).cast(),
                std::mem::size_of::<T>(),
                null_mut(),
                null_mut(),
            )
        })
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.pointer());
        }
    }
}
fn check(result: i32) -> io::Result<()> {
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
    value.encode_wide().chain([0]).collect()
}
fn ordinary(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(value) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{value}"));
    }
    text.strip_prefix(r"\\?\")
        .map(PathBuf::from)
        .unwrap_or(path)
}
fn resolve(program: &str, cwd: &Path) -> io::Result<PathBuf> {
    let program = Path::new(program);
    let bases: Vec<_> = if program.is_absolute() {
        vec![program.to_owned()]
    } else if program.components().count() > 1 {
        vec![cwd.join(program)]
    } else {
        std::iter::once(cwd.to_owned())
            .chain(std::env::split_paths(
                &std::env::var_os("PATH").unwrap_or_default(),
            ))
            .map(|dir| {
                if dir.is_absolute() {
                    dir.join(program)
                } else {
                    cwd.join(dir).join(program)
                }
            })
            .collect()
    };
    for base in bases {
        let candidates = if base.extension().is_none() {
            vec![
                base.with_extension("exe"),
                base.with_extension("com"),
                base.with_extension("cmd"),
                base.with_extension("bat"),
                base,
            ]
        } else {
            vec![base]
        };
        for candidate in candidates {
            match candidate.metadata() {
                Ok(meta) if meta.is_file() => return std::path::absolute(&candidate).map(ordinary),
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Command executable not found",
    ))
}
fn quote(argument: &str) -> String {
    if !argument.is_empty() && !argument.chars().any(|c| c.is_whitespace() || c == '"') {
        return argument.into();
    }
    let mut result = String::from("\"");
    let mut slashes = 0;
    for c in argument.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        if c == '"' {
            result.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
        } else {
            result.extend(std::iter::repeat_n('\\', slashes));
        }
        result.push(c);
        slashes = 0;
    }
    result.extend(std::iter::repeat_n('\\', slashes * 2));
    result.push('"');
    result
}

fn pipe(attributes: &mut SecurityAttributes) -> io::Result<(Handle, Handle)> {
    let mut read = null_mut();
    let mut write = null_mut();
    check(unsafe { CreatePipe(&mut read, &mut write, attributes, 0) })?;
    let read = Handle::new(read)?;
    let write = Handle::new(write)?;
    check(unsafe { SetHandleInformation(read.0, 1, 0) })?;
    Ok((read, write))
}
pub(super) struct Pipe(File, bool);
impl PollRead for Pipe {
    fn source_closed(&self) -> bool {
        self.1
    }
    fn poll_read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let mut available = 0;
        if unsafe {
            PeekNamedPipe(
                self.0.as_raw_handle(),
                null_mut(),
                0,
                null_mut(),
                &mut available,
                null_mut(),
            )
        } == 0
        {
            let error = io::Error::last_os_error();
            return if error.raw_os_error() == Some(109) {
                Ok(0)
            } else {
                Err(error)
            };
        }
        if available == 0 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let count = bytes.len().min(available as usize);
        self.0.read(&mut bytes[..count])
    }
}
pub(super) struct Child {
    process: Handle,
    job: Handle,
    pub stdout: Pipe,
    pub stderr: Pipe,
    terminated: bool,
}
impl Child {
    pub fn spawn(request: &CommandRequest) -> io::Result<Self> {
        Self::spawn_observed(request, |_| {})
    }
    fn spawn_observed(request: &CommandRequest, observe: impl FnOnce(&Handle)) -> io::Result<Self> {
        let cwd = ordinary(request.cwd.canonicalize()?);
        let mut image = resolve(&request.argv[0], &cwd)?;
        let mut argv = request.argv.clone();
        if image
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
        {
            let script = image.to_str().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "Non-Unicode batch path")
            })?;
            let (_, args) = work_operations::execution::command::run::windows_batch_command_line(
                script,
                &argv[1..],
            )
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid batch argument"))?;
            let comspec = std::env::var("COMSPEC")
                .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "COMSPEC is unavailable"))?;
            image = resolve(&comspec, &cwd)?;
            if !image
                .file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("cmd.exe"))
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "COMSPEC must name cmd.exe",
                ));
            }
            argv = std::iter::once(image.to_string_lossy().into_owned())
                .chain(args)
                .collect();
        }
        let cmd = image
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("cmd.exe"));
        let command = if cmd {
            if let Some(position) = argv.iter().position(|a| a.eq_ignore_ascii_case("/c")) {
                if argv.len() != position + 2 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "CMD requires one command string",
                    ));
                }
                let prefix = std::iter::once(quote(&image.to_string_lossy()))
                    .chain(argv[1..=position].iter().map(|s| quote(s)))
                    .collect::<Vec<_>>()
                    .join(" ");
                let script = &argv[position + 1];
                if argv[1..position]
                    .iter()
                    .any(|a| a.eq_ignore_ascii_case("/s"))
                {
                    format!("{prefix} \"{script}\"")
                } else {
                    format!("{prefix} {script}")
                }
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Interactive CMD is unsupported",
                ));
            }
        } else {
            std::iter::once(quote(&image.to_string_lossy()))
                .chain(argv[1..].iter().map(|s| quote(s)))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let job = Handle::new(unsafe { CreateJobObjectW(null_mut(), std::ptr::null()) })?;
        let mut limits: ExtendedLimits = unsafe { std::mem::zeroed() };
        limits.basic.flags = 0x2000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, no token/ACL policy.
        check(unsafe {
            SetInformationJobObject(
                job.0,
                9,
                (&mut limits as *mut ExtendedLimits).cast(),
                std::mem::size_of::<ExtendedLimits>() as u32,
            )
        })?;
        let mut security = SecurityAttributes {
            length: std::mem::size_of::<SecurityAttributes>() as u32,
            descriptor: null_mut(),
            inherit: 1,
        };
        let (out_read, out_write) = pipe(&mut security)?;
        let (err_read, err_write) = pipe(&mut security)?;
        let input = Handle::new(unsafe {
            CreateFileW(
                wide(std::ffi::OsStr::new("NUL")).as_ptr(),
                0x80000000,
                3,
                &mut security,
                3,
                0,
                null_mut(),
            )
        })?;
        let mut attributes = Attributes::new()?;
        let mut handles = [input.0, out_write.0, err_write.0];
        let mut jobs = [job.0];
        attributes.set(0x00020002, &mut handles)?; // HANDLE_LIST
        attributes.set(0x0002000d, &mut jobs)?; // JOB_LIST attached during process creation.
        let mut startup: StartupEx = unsafe { std::mem::zeroed() };
        startup.startup.size = std::mem::size_of::<StartupEx>() as u32;
        startup.startup.flags = 0x100;
        startup.startup.input = input.0;
        startup.startup.output = out_write.0;
        startup.startup.error = err_write.0;
        startup.attributes = attributes.pointer();
        let mut info: ProcessInformation = unsafe { std::mem::zeroed() };
        let mut line = wide(std::ffi::OsStr::new(&command));
        // SAFETY: live aligned ABI objects, NUL terminated paths, explicit stream handles.
        // Null environment inherits the host environment. No alternate token is used.
        check(unsafe {
            CreateProcessW(
                wide(image.as_os_str()).as_ptr(),
                line.as_mut_ptr(),
                null_mut(),
                null_mut(),
                1,
                0x08080400,
                null_mut(),
                wide(cwd.as_os_str()).as_ptr(),
                &mut startup.startup,
                &mut info,
            )
        })?;
        let process = Handle::new(info.process)?;
        let _thread = Handle::new(info.thread)?;
        observe(&out_write);
        Ok(Self {
            process,
            job,
            stdout: Pipe(out_read.file(), false),
            stderr: Pipe(err_read.file(), false),
            terminated: false,
        })
    }
    pub fn poll(&mut self) -> io::Result<Option<(Option<i32>, Option<i32>)>> {
        match unsafe { WaitForSingleObject(self.process.0, 0) } {
            0 => {
                let mut code = 0;
                check(unsafe { GetExitCodeProcess(self.process.0, &mut code) })?;
                Ok(Some((Some(code as i32), None)))
            }
            258 => Ok(None),
            _ => Err(io::Error::last_os_error()),
        }
    }
    pub fn terminate(&mut self) -> io::Result<()> {
        if self.terminated {
            return Ok(());
        }
        #[cfg(test)]
        let terminate_started = std::time::Instant::now();
        check(unsafe { TerminateJobObject(self.job.0, 1) })?;
        #[cfg(test)]
        let terminate_call_ms = terminate_started.elapsed().as_millis();
        #[cfg(test)]
        let leader_started = std::time::Instant::now();
        let leader_wait_result = unsafe { WaitForSingleObject(self.process.0, 500) };
        #[cfg(test)]
        eprintln!(
            "Windows Job termination: terminate_call_ms={terminate_call_ms} leader_wait_ms={} leader_wait_result={leader_wait_result}",
            leader_started.elapsed().as_millis()
        );
        if leader_wait_result != 0 {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Process termination unverified",
            ));
        }
        let started = std::time::Instant::now();
        loop {
            let mut accounting: Accounting = unsafe { std::mem::zeroed() };
            check(unsafe {
                QueryInformationJobObject(
                    self.job.0,
                    1,
                    (&mut accounting as *mut Accounting).cast(),
                    std::mem::size_of::<Accounting>() as u32,
                    null_mut(),
                )
            })?;
            if accounting.active == 0 {
                #[cfg(test)]
                eprintln!(
                    "Windows Job termination: job_wait_ms={} active_processes=0",
                    started.elapsed().as_millis()
                );
                break;
            }
            if started.elapsed() >= std::time::Duration::from_millis(500) {
                #[cfg(test)]
                eprintln!(
                    "Windows Job termination: job_wait_ms={} active_processes={}",
                    started.elapsed().as_millis(),
                    accounting.active
                );
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Job descendants still active",
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // Windows may inherit transient pipe handles into unrelated concurrent children.
        // Native EOF and verified owned-source completion are distinct observations:
        // only an empty read after every Job producer has exited completes this source.
        self.stdout.1 = true;
        self.stderr.1 = true;
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
    use crate::process::host::tests::fixture;
    use crate::process::host::{Status, run};
    use std::{fs, sync::atomic::AtomicBool};

    #[test]
    fn batch_uses_comspec_and_preserves_normal_cwd_and_spaces() {
        let mut request = fixture(serde_json::json!({"mode":"argv"}));
        let script = request.cwd.join("測試 tool.cmd");
        fs::write(
            &script,
            "@echo off\r\necho %~1\r\necho %cd%\r\nexit /b 9\r\n",
        )
        .unwrap();
        request.argv = vec![script.to_string_lossy().into_owned(), "a b 中文".into()];
        let capture = run(&request, &AtomicBool::new(false));
        assert_eq!(capture.status, Status::Exited);
        assert_eq!(capture.exit_code, Some(9));
        assert!(capture.cleanup_error.is_none());
        let text = String::from_utf8_lossy(&capture.stdout.full.bytes);
        // CMD's native output encoding is preserved as bytes, not silently transcoded.
        assert!(text.contains("a b"));
        assert!(capture.stdout.full.complete());
    }
    #[test]
    fn completed_job_drains_owned_output_even_when_an_unrelated_writer_handle_is_retained() {
        use super::*;
        use crate::process::host::Stream;
        use std::time::{Duration, Instant};
        let request = fixture(serde_json::json!({"mode":"argv"}));
        let mut retained = None;
        let mut child = Child::spawn_observed(&request, |writer| {
            let mut duplicate = null_mut();
            check(unsafe {
                DuplicateHandle(
                    GetCurrentProcess(),
                    writer.0,
                    GetCurrentProcess(),
                    &mut duplicate,
                    0,
                    0,
                    2,
                )
            })
            .unwrap();
            retained = Some(Handle::new(duplicate).unwrap());
        })
        .unwrap();
        let started = Instant::now();
        while child.poll().unwrap().is_none() {
            assert!(started.elapsed() < Duration::from_secs(3));
            std::thread::sleep(Duration::from_millis(5));
        }
        child.terminate().unwrap();
        let mut output = Stream::default();
        output.drain(&mut child.stdout);
        assert!(
            !output.full.eof,
            "The retained foreign handle prevents native pipe EOF"
        );
        assert!(output.full.source_closed && output.full.complete());
        assert!(String::from_utf8_lossy(&output.full.bytes).contains("HOST_DATA:"));
        assert!(started.elapsed() < Duration::from_secs(3));
        drop(retained);
    }
    #[test]
    fn native_pipe_eof_and_descriptor_errors_are_distinct() {
        use super::*;
        use crate::process::host::Stream;
        use std::io::Write;
        let mut attributes = SecurityAttributes {
            length: std::mem::size_of::<SecurityAttributes>() as u32,
            descriptor: null_mut(),
            inherit: 1,
        };
        let (reader, writer) = pipe(&mut attributes).unwrap();
        let mut writer = writer.file();
        writer.write_all(b"actual\0bytes").unwrap();
        drop(writer);
        let mut reader = Pipe(reader.file(), false);
        let mut stream = Stream::default();
        stream.drain(&mut reader);
        assert_eq!(stream.full.bytes, b"actual\0bytes");
        assert!(stream.full.eof && stream.full.complete() && !stream.full.read_failed);
        let mut file = std::env::temp_dir();
        file.push(format!("work-pipe-error-{}.txt", std::process::id()));
        fs::write(&file, b"regular file is not a pipe").unwrap();
        let mut reader = Pipe(File::open(file).unwrap(), false);
        let mut stream = Stream::default();
        stream.drain(&mut reader);
        assert!(stream.full.read_failed && !stream.full.eof && !stream.full.complete());
    }
}
