//! Checkout CLI for trusted Python tests.
use kiryl::{
    command::{Operation, Request},
    executor::Limits,
};
use serde_json::{Value, json};
use std::{
    env, io,
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
    let operation = match command.as_str() {
        "run" => Operation::Run,
        "replay" => Operation::Replay(PathBuf::from(
            finding.ok_or_else(|| invalid("missing finding"))?,
        )),
        _ => Operation::Minimize(PathBuf::from(
            finding.ok_or_else(|| invalid("missing finding"))?,
        )),
    };
    let flag = Arc::new(AtomicBool::new(false));
    let handler = Arc::clone(&flag);
    ctrlc::set_handler(move || handler.store(true, Ordering::Relaxed))?;
    kiryl::command::execute(
        Request {
            operation,
            python,
            selection,
            limits,
            seed,
            store,
        },
        context,
        flag,
    )?
    .publish(report_path.as_deref())
    .map_err(Into::into)
}

fn main() {
    let started = std::time::Instant::now();
    let args: Vec<_> = env::args_os().skip(1).collect();
    let mut context = json!({"schema_version":1,"targets":[],"selection_complete":false});
    let code = run(&mut context).unwrap_or_else(|error| {
        eprintln!("kiryl: {error}");
        let completion =
            kiryl::report::record_error(&mut context, error.as_ref(), started.elapsed());
        let bytes = serde_json::to_vec_pretty(&context).unwrap_or_default();
        if let Some([_, report_path]) = args.windows(2).find(|pair| pair[0] == "--report") {
            if let Err(error) =
                kiryl::report::publish_output(Some(Path::new(report_path)), &context)
            {
                eprintln!("kiryl: error report persistence failed: {error}");
                println!("{}", String::from_utf8_lossy(&bytes));
                return kiryl::report::Completion::Infrastructure.exit_code();
            }
        } else {
            println!("{}", String::from_utf8_lossy(&bytes));
        }
        completion.exit_code()
    });
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::milliseconds;
    #[rstest::rstest]
    fn reject_durations_below_clock_resolution(#[values("0.00001", "NaN", "-1")] input: &str) {
        assert!(milliseconds(input).is_err());
    }
    #[test]
    fn accept_duration_at_clock_resolution() {
        assert_eq!(milliseconds("0.001").unwrap(), 1);
    }
}
