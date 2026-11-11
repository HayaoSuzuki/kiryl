//! Checkout CLI for trusted Python tests.
use kiryl::{
    campaign::{self, CampaignConfig},
    executor::{Executor, ExecutorConfig, Limits},
};
use serde_json::{Value, json};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
fn invalid(message: &str) -> io::Error {
    io::Error::other(message)
}
fn milliseconds(value: &str) -> io::Result<u64> {
    let seconds: f64 = value
        .parse()
        .map_err(|_| invalid("duration must be seconds"))?;
    if !seconds.is_finite() || seconds < 0.001 || seconds > 1e12 {
        return Err(invalid("duration must be finite and positive"));
    }
    std::time::Duration::from_secs_f64(seconds)
        .as_millis()
        .try_into()
        .map_err(|_| invalid("duration too large"))
}
#[allow(clippy::too_many_lines)] // Single CLI dispatch; worker and campaign semantics remain encapsulated.
fn run(context: &mut Value) -> Result<i32, Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".into());
    context["command"] = json!(command);
    if ["help", "--help", "-h"].contains(&command.as_str()) {
        println!(
            "kiryl run [PATH_OR_NODE...] [-k EXPR] [-m MARKER] [--duration SECONDS] [--python PATH] [--best-effort] [--store DIR] [--report FILE]\nkiryl replay FINDING [OPTIONS]\nkiryl minimize FINDING [OPTIONS]"
        );
        return Ok(0);
    }
    if !["run", "replay", "minimize"].contains(&command.as_str()) {
        return Err(invalid("unknown command").into());
    }
    let mut python = PathBuf::from("python3");
    let mut store = PathBuf::from(".kiryl");
    let mut report_path = None;
    let mut limits = Limits {
        case_ms: 10_000,
        slice_ms: 1000,
        retained_output_bytes: 65536,
        ..Limits::default()
    };
    let mut selection = vec![];
    let mut finding = None;
    let mut seed = 0;
    while let Some(arg) = args.next() {
        if arg == "--best-effort" {
            limits.allow_best_effort = true;
            continue;
        }
        if arg.starts_with('-') {
            let value = args.next().ok_or_else(|| invalid("missing option value"))?;
            match arg.as_str() {
                "--python" => python = PathBuf::from(value),
                "--store" => store = PathBuf::from(value),
                "--report" => report_path = Some(PathBuf::from(value)),
                "--duration" => limits.campaign_ms = Some(milliseconds(&value)?),
                "--case-timeout" => limits.case_ms = milliseconds(&value)?,
                "--slice" => limits.slice_ms = milliseconds(&value)?,
                "--memory-mib" => {
                    limits.tree_memory_bytes = value
                        .parse::<u64>()?
                        .checked_mul(1024 * 1024)
                        .ok_or_else(|| invalid("memory overflow"))?;
                }
                "--output-bytes" => limits.retained_output_bytes = value.parse()?,
                "--seed" => seed = value.parse()?,
                "-k" | "-m" => {
                    selection.push(arg);
                    selection.push(value);
                }
                _ => return Err(invalid("unknown option").into()),
            }
        } else if command == "run" {
            selection.push(arg);
        } else if finding.replace(arg).is_some() {
            return Err(invalid("expected one finding").into());
        }
    }
    if limits.tree_memory_bytes == 0 || limits.retained_output_bytes > 16 * 1024 * 1024 {
        return Err(
            invalid("memory must be positive and retained output cannot exceed 16 MiB").into(),
        );
    }
    if command != "run" && selection.is_empty() {
        let path = PathBuf::from(finding.as_ref().ok_or_else(|| invalid("missing finding"))?);
        let parent = path
            .parent()
            .ok_or_else(|| invalid("finding has no parent"))?;
        let manifest: Value = serde_json::from_slice(&fs::read(parent.join("manifest.json"))?)?;
        selection =
            serde_json::from_value(manifest["identity"]["environment"]["selection"].clone())?;
    }
    let flag = Arc::new(AtomicBool::new(false));
    let handler = Arc::clone(&flag);
    ctrlc::set_handler(move || handler.store(true, Ordering::Relaxed))?;
    let requested_limits = serde_json::to_value(&limits)?;
    context["effective_limits"] = requested_limits.clone();
    context["selection"] = json!(selection);
    let overall_start = std::time::Instant::now();
    let discovered = kiryl::executor::collect(&python, &selection, &limits, &flag)?;
    if let Some(budget) = limits.campaign_ms.as_mut() {
        *budget = budget
            .saturating_sub(u64::try_from(overall_start.elapsed().as_millis()).unwrap_or(u64::MAX));
    }
    context["environment"] = discovered["environment"].clone();
    if let Some(known) = discovered["nodeids"].as_array() {
        context["targets"] = json!(
            known
                .iter()
                .map(|target| json!({"target":target,"status":"unexecuted"}))
                .collect::<Vec<_>>()
        );
    }
    if discovered
        .get("error")
        .is_some_and(|value| !value.is_null())
    {
        return Err(invalid(&format!(
            "pytest collection failed; no targets executed: {}",
            discovered["error"]
        ))
        .into());
    }
    context["environment"] = discovered["environment"].clone();
    let targets: Vec<String> = serde_json::from_value(discovered["nodeids"].clone())?;
    context["selection_complete"] = json!(true);
    if targets.is_empty() {
        return Err(invalid("selection contains no supported Hypothesis tests").into());
    }
    let identity = json!({"environment":discovered["environment"],"sources":discovered["sources"],"implementation_sources":discovered["implementation_sources"],"selection":selection});
    context["environment_identity"] = identity.clone();
    context["targets"] = json!(
        targets
            .iter()
            .map(|target| json!({"target":target,"status":"unexecuted"}))
            .collect::<Vec<_>>()
    );
    let environment_identity = identity.clone();
    let remaining_execution_limits = serde_json::to_value(&limits)?;
    let replay_python = python.clone();
    let mut report = if command == "run" {
        campaign::run(
            &CampaignConfig {
                python,
                pytest_args: selection,
                targets,
                identity,
                store,
                limits,
                seed,
            },
            &flag,
        )?
    } else {
        let original = fs::canonicalize(finding.ok_or_else(|| invalid("missing finding"))?)?;
        let directory = original
            .parent()
            .ok_or_else(|| invalid("finding has no parent"))?
            .to_owned();
        let _campaign_lock = kiryl::artifacts::ArtifactStore::open(
            directory
                .parent()
                .ok_or_else(|| invalid("missing campaign store"))?,
        )?;
        let manifest: Value = serde_json::from_slice(&fs::read(directory.join("manifest.json"))?)?;
        if manifest["identity"]["environment"] != identity {
            return Err(invalid("source, selection or environment mismatch").into());
        }
        let config = ExecutorConfig {
            python,
            pytest_args: selection,
            invocations: vec![],
            limits,
            seed,
            search: false,
            feedback: true,
            baseline: false,
            output: directory,
            identity: manifest["identity"].clone(),
        };
        serde_json::to_value(
            Executor::new(flag).operation(
                &config,
                original
                    .to_str()
                    .ok_or_else(|| invalid("non UTF-8 finding path"))?,
                &command,
            )?,
        )?
    };
    let best_effort = requested_limits["allow_best_effort"]
        .as_bool()
        .unwrap_or(false);
    report["effective_limits"] = requested_limits;
    report["remaining_execution_limits"] = remaining_execution_limits;
    report["environment_identity"] = environment_identity;
    report["schema_version"] = json!(1);
    report["command"] = json!(command);
    report["total_elapsed_ms"] = json!(overall_start.elapsed().as_millis());
    kiryl::report::present_findings(
        &mut report,
        &env::current_exe()?,
        &replay_python,
        best_effort,
    )?;
    let bytes = serde_json::to_vec_pretty(&report)?;
    if let Some(path) = report_path {
        publish(&path, &bytes)?;
    } else {
        println!("{}", String::from_utf8(bytes)?);
    }
    Ok(if command == "run" {
        campaign::exit_code(&report)
    } else if report["interrupted"] == true {
        130
    } else if report["outcomes"]
        .as_array()
        .is_some_and(|values| values.iter().any(|value| value["kind"] == "infrastructure"))
    {
        2
    } else {
        1
    })
}
fn publish(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let pending = path.with_extension(format!("pending-{}", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)?;
    let result = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    let result = result.and_then(|()| fs::rename(&pending, path));
    if result.is_err() {
        let _ = fs::remove_file(pending);
    }
    result
}
fn main() {
    let started = std::time::Instant::now();
    let args: Vec<_> = env::args_os().skip(1).collect();
    let mut context = json!({"schema_version":1,"targets":[],"selection_complete":false});
    let code = run(&mut context).unwrap_or_else(|error| {
        eprintln!("kiryl: {error}");
        let kind = error.downcast_ref::<io::Error>().map(io::Error::kind);
        let code = match kind {
            Some(io::ErrorKind::Interrupted) => 130,
            Some(io::ErrorKind::TimedOut) => 0,
            _ => 2,
        };
        context["status"] = json!(match code {
            130 => "interrupted",
            0 => "campaign_cancelled",
            _ => "infrastructure",
        });
        context["interrupted"] = json!(code == 130);
        context["elapsed_ms"] = json!(started.elapsed().as_millis());
        context["infrastructure_errors"] = if code == 2 {
            json!([error.to_string().chars().take(16384).collect::<String>()])
        } else {
            json!([])
        };
        let bytes = serde_json::to_vec_pretty(&context).unwrap_or_default();
        if let Some([_, report_path]) = args.windows(2).find(|pair| pair[0] == "--report") {
            if let Err(error) = publish(Path::new(report_path), &bytes) {
                eprintln!("kiryl: error report persistence failed: {error}");
                println!("{}", String::from_utf8_lossy(&bytes));
                return 2;
            }
        } else {
            println!("{}", String::from_utf8_lossy(&bytes));
        }
        code
    });
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::milliseconds;
    #[test]
    fn reject_durations_below_clock_resolution() {
        assert!(milliseconds("0.00001").is_err());
        assert!(milliseconds("NaN").is_err());
        assert!(milliseconds("-1").is_err());
        assert_eq!(milliseconds("0.001").unwrap(), 1);
    }
}
