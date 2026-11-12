//! Finding presentation owns user-facing values and replay command construction.
use crate::executor::{OutcomeKind, Reproduction};
use serde_json::{Value, json};
use std::{io, path::Path};

/// Add available rendered values and argument arrays suitable for fresh operations.
/// # Errors
/// Returns missing/corrupt original records rather than presenting invented evidence.
pub fn present_findings(
    report: &mut Value,
    executable: &Path,
    python: &Path,
    best_effort: bool,
) -> io::Result<()> {
    let limits = report["effective_limits"].clone();
    annotate(report, executable, python, best_effort, &limits)?;
    if let Some(targets) = report["targets"].as_array_mut() {
        for target in targets {
            annotate(
                &mut target["report"],
                executable,
                python,
                best_effort,
                &limits,
            )?;
        }
    }
    Ok(())
}
fn annotate(
    report: &mut Value,
    executable: &Path,
    python: &Path,
    best_effort: bool,
    limits: &Value,
) -> io::Result<()> {
    let Some(outcomes) = report["outcomes"].as_array_mut() else {
        return Ok(());
    };
    for outcome in outcomes {
        let Some(path) = outcome["original"].as_str().map(str::to_owned) else {
            continue;
        };
        let description = crate::artifacts::describe_original(Path::new(&path))?;
        outcome["input_values"] = json!(description.rendered);
        outcome["failure_fingerprint"] = json!(description.fingerprint);
        for command in [Reproduction::Replay, Reproduction::Minimize] {
            if command == Reproduction::Minimize && !description.replayable {
                continue;
            }
            let mut args = vec![
                json!(executable),
                json!(command),
                json!(path),
                json!("--python"),
                json!(python),
            ];
            if best_effort {
                args.push(json!("--best-effort"));
            }
            if let Some(memory) = limits["tree_memory_bytes"].as_u64() {
                args.extend([
                    json!("--memory-mib"),
                    json!((memory / (1024 * 1024)).to_string()),
                ]);
            }
            if let Some(case) = limits["case_ms"].as_u64() {
                args.extend([
                    json!("--case-timeout"),
                    json!(
                        std::time::Duration::from_millis(case)
                            .as_secs_f64()
                            .to_string()
                    ),
                ]);
            }
            let key = match command {
                Reproduction::Replay => "replay_command",
                Reproduction::Minimize => "minimize_command",
            };
            outcome[key] = json!(args);
        }
    }
    Ok(())
}

/// Completion severity is independent of report nesting and wire encoding.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Completion {
    Clean,
    Finding,
    Infrastructure,
    Interrupted,
}
impl Completion {
    /// Preserve the strongest observed result; interruption has process priority.
    #[must_use]
    pub fn combine(self, other: Self) -> Self {
        self.max(other)
    }
    /// Stable CLI exit contract shared by search and standalone operations.
    #[must_use]
    pub fn exit_code(self) -> i32 {
        match self {
            Self::Clean => 0,
            Self::Finding => 1,
            Self::Infrastructure => 2,
            Self::Interrupted => 130,
        }
    }
}
/// Classify a recorded observation without forgetting an unresolved active input.
#[must_use]
pub fn completion(kind: &OutcomeKind, has_original: bool) -> Completion {
    match kind {
        OutcomeKind::Infrastructure => Completion::Infrastructure,
        OutcomeKind::CampaignCancelled if has_original => Completion::Finding,
        OutcomeKind::Pass
        | OutcomeKind::Reject
        | OutcomeKind::Skip
        | OutcomeKind::CampaignCancelled => Completion::Clean,
        _ => Completion::Finding,
    }
}
/// A terminal target no longer receives slices; expiry alone does not stop it.
#[must_use]
pub fn stops_search(kind: &OutcomeKind) -> bool {
    !matches!(
        kind,
        OutcomeKind::Pass | OutcomeKind::Reject | OutcomeKind::CampaignCancelled
    )
}
/// Read either report shape through one outcome/exit contract.
#[must_use]
pub fn exit_code(report: &Value) -> i32 {
    report_completion(report).exit_code()
}
fn report_completion(report: &Value) -> Completion {
    if report["interrupted"] == true {
        return Completion::Interrupted;
    }
    let mut result = Completion::Clean;
    if report["status"].as_str().map(OutcomeKind::from) == Some(OutcomeKind::Infrastructure) {
        result = Completion::Infrastructure;
    }
    if let Some(outcomes) = report["outcomes"].as_array() {
        for outcome in outcomes {
            result = result.combine(completion(
                &outcome["kind"]
                    .as_str()
                    .map_or(OutcomeKind::Infrastructure, OutcomeKind::from),
                !outcome["original"].is_null(),
            ));
        }
    }
    if let Some(targets) = report["targets"].as_array() {
        for target in targets {
            result = result.combine(report_completion(&target["report"]));
        }
    }
    result
}

/// Record an early command failure while retaining the context collected so far.
/// Budget expiry before an active case is clean cancellation; no finding is invented.
/// Diagnostics are bounded, and interruption takes the same priority as normal results.
pub fn record_error(
    context: &mut Value,
    error: &(dyn std::error::Error + 'static),
    elapsed: std::time::Duration,
) -> Completion {
    let completion = match error.downcast_ref::<io::Error>().map(io::Error::kind) {
        Some(io::ErrorKind::Interrupted) => Completion::Interrupted,
        Some(io::ErrorKind::TimedOut) => Completion::Clean,
        _ => Completion::Infrastructure,
    };
    context["status"] = json!(match completion {
        Completion::Interrupted => OutcomeKind::Interrupted,
        Completion::Clean => OutcomeKind::CampaignCancelled,
        Completion::Infrastructure | Completion::Finding => OutcomeKind::Infrastructure,
    });
    context["interrupted"] = json!(completion == Completion::Interrupted);
    context["elapsed_ms"] = json!(elapsed.as_millis());
    context["infrastructure_errors"] = if completion == Completion::Infrastructure {
        json!([error.to_string().chars().take(16384).collect::<String>()])
    } else {
        json!([])
    };
    completion
}

/// Publish one complete report, atomically replacing a requested file.
/// Failed file writes leave the previous report intact and remove the pending file.
/// # Errors
/// Returns serialization, write, sync or rename failures.
pub fn publish_output(path: Option<&Path>, report: &Value) -> io::Result<()> {
    use std::io::Write;
    let bytes = serde_json::to_vec_pretty(report)?;
    let Some(path) = path else {
        println!("{}", String::from_utf8_lossy(&bytes));
        return Ok(());
    };
    let pending = path.with_extension(format!("pending-{}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)?;
    let result = file.write_all(&bytes).and_then(|()| file.sync_all());
    drop(file);
    let result = result.and_then(|()| std::fs::rename(&pending, path));
    if result.is_err() {
        let _ = std::fs::remove_file(pending);
    }
    result
}
