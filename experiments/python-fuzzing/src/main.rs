// Copyright (c) 2026 kiryl contributors
//! Experimental CLI. All report status is returned through the exit code.
use fuzz_probe::controller::{Controller, ProbeConfig};
use std::{
    env, fs, io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 5 || args[0] != "run" || args[1] != "--config" || args[3] != "--out" {
        return Err(
            io::Error::other("usage: fuzz-probe run --config CONFIG --out DIRECTORY").into(),
        );
    }
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&args[2])?)?;
    value["output"] = serde_json::to_value(PathBuf::from(&args[4]))?;
    let config: ProbeConfig = serde_json::from_value(value)?;
    let interrupted = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&interrupted);
    ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed))?;
    let output = config.output.clone();
    let report = Controller::new(interrupted).run(config)?;
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
