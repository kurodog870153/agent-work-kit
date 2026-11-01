//! Test Host: execute an exact Work binding and retain rereadable result bytes.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

pub struct HostResult {
    pub report: Value,
    pub directory: PathBuf,
    pub output: Output,
}

pub fn execute(root: &Path, binding: &Value) -> HostResult {
    let command = &binding["command"];
    assert_eq!(command["mode"], "argv");
    let argv = command["argv"].as_array().expect("bound argv");
    let mut process = Command::new(argv[0].as_str().expect("executable"));
    for argument in &argv[1..] {
        process.arg(argument.as_str().expect("argument"));
    }
    let cwd = binding["execution"]["working_directory"]
        .as_str()
        .unwrap_or(".");
    let output = process
        .current_dir(root.join(cwd))
        .output()
        .expect("Host execution");
    let relative = format!(
        "host-evidence/{}/{}",
        binding["attempt_id"].as_str().unwrap(),
        binding["record_id"].as_str().unwrap()
    );
    let directory = root.join(&relative);
    fs::create_dir_all(&directory).unwrap();
    let artifact = |file: &str, bytes: &[u8]| {
        json!({"file":file,"size_bytes":bytes.len(),
            "sha256":work_infrastructure::fixture_support::raw_sha256(bytes),"complete":true})
    };
    fs::write(directory.join("stdout.bin"), &output.stdout).unwrap();
    fs::write(directory.join("stderr.bin"), &output.stderr).unwrap();
    let mut reports = serde_json::Map::new();
    for (position, path) in binding["capture"]["report_paths"]
        .as_array()
        .expect("bound reports")
        .iter()
        .enumerate()
    {
        let path = path.as_str().unwrap();
        let bytes = fs::read(root.join(cwd).join(path)).expect("declared report");
        let file = format!("report-{position:04}.bin");
        fs::write(directory.join(&file), &bytes).unwrap();
        reports.insert(
            path.into(),
            json!({"status":"available","artifact":artifact(&file, &bytes)}),
        );
    }
    let captures = json!({"stdout":artifact("stdout.bin", &output.stdout),
        "stderr":artifact("stderr.bin", &output.stderr),"reports":reports});
    let raw = serde_json::to_vec(&captures).unwrap();
    fs::write(directory.join("result.json"), &raw).unwrap();
    let report = json!({"schema":"work-host-execution-report",
        "task_id":binding["task_id"],"attempt_id":binding["attempt_id"],
        "record_id":binding["record_id"],
        "task_collection_sha256":binding["task_collection_sha256"],
        "authorization_sha256":binding["authorization_sha256"],
        "file_version":binding["file_version"],"command_sha256":binding["command_sha256"],
        "result_origin":"host_reported","result_reference":format!("{relative}/result.json"),
        "result_sha256":work_infrastructure::fixture_support::raw_sha256(&raw),
        "status":"exited","exit_code":output.status.code().expect("known exit"),
        "output_complete":true});
    HostResult {
        report,
        directory,
        output,
    }
}
