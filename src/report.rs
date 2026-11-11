//! Finding presentation owns user-facing values and replay command construction.
use serde_json::{Value, json};
use std::{
    fs::File,
    io::{self, Read},
    path::Path,
};

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
        let mut bytes = vec![];
        File::open(&path)?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 1024 * 1024 {
            return Err(io::Error::other("original presentation exceeds limit"));
        }
        let original: Value = serde_json::from_slice(&bytes)?;
        outcome["input_values"] = original["observation"]["rendered"].clone();
        outcome["failure_fingerprint"] = original["observation"]["fingerprint"].clone();
        for command in ["replay", "minimize"] {
            if command == "minimize" && original["observation"]["replay_token"].is_null() {
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
            outcome[format!("{command}_command")] = json!(args);
        }
    }
    Ok(())
}
