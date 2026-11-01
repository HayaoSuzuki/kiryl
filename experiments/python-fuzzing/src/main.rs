// Copyright (c) 2026 kiryl contributors
//! Experimental CLI. All report status is returned through the exit code.
use fuzz_probe::controller::{Controller, Outcome, ProbeConfig, ProbeReport};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

fn configured(path: &Path, output: &Path) -> Result<ProbeReport, Box<dyn std::error::Error>> {
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
    if !value.is_object() {
        return Err(io::Error::other("configuration must be a JSON object").into());
    }
    value["output"] = serde_json::to_value(output)?;
    let config: ProbeConfig = serde_json::from_value(value)?;
    let interrupted = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&interrupted);
    ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed))?;
    Ok(Controller::new(interrupted).run(config)?)
}
fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 5 || args[0] != "run" || args[1] != "--config" || args[3] != "--out" {
        return Err(
            io::Error::other("usage: fuzz-probe run --config CONFIG --out DIRECTORY").into(),
        );
    }
    let output = PathBuf::from(&args[4]);
    fs::create_dir_all(&output)?;
    let report = configured(Path::new(&args[2]), &output).unwrap_or_else(|error| ProbeReport {
        version: 1,
        outcomes: vec![Outcome {
            test_id: String::new(),
            case_id: 0,
            kind: "infrastructure".into(),
            original: None,
            derived: vec![],
            verification: "pending".into(),
            detail: Some(error.to_string()),
        }],
        ..ProbeReport::default()
    });
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report.exit_code())
}
fn main() {
    let code = run().unwrap_or_else(|error| {
        eprintln!("{error}");
        2
    });
    std::process::exit(code);
}
