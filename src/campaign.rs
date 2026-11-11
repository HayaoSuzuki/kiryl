//! Equal-time allocation: charge actual slice wall time, including cooperative overrun.
use crate::executor::{Executor, ExecutorConfig, Invocation, Limits};
use serde_json::{Value, json};
use std::{
    io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

/// Select the least charged unfinished target, keeping overshoot as future debt.
#[must_use]
pub fn choose_next(charged: &[u128], stopped: &[bool]) -> Option<usize> {
    charged
        .iter()
        .enumerate()
        .filter(|(i, _)| !stopped[*i])
        .min_by_key(|(_, time)| **time)
        .map(|(i, _)| i)
}
/// Product configuration hides worker protocol details from CLI parsing.
pub struct CampaignConfig {
    pub python: PathBuf,
    pub pytest_args: Vec<String>,
    pub targets: Vec<String>,
    pub identity: Value,
    pub store: PathBuf,
    pub limits: Limits,
    pub seed: u64,
}
/// Run one target at a time until all stop, interruption, or the shared budget ends.
/// # Errors
/// Returns worker or storage failures without deleting existing evidence.
pub fn run(config: &CampaignConfig, interrupted: &Arc<AtomicBool>) -> io::Result<Value> {
    let _campaign_lock = crate::artifacts::ArtifactStore::open(&config.store)?;
    let started = Instant::now();
    let mut charged = vec![0; config.targets.len()];
    let mut stopped = vec![false; config.targets.len()];
    let mut targets: Vec<Value> = config
        .targets
        .iter()
        .map(|target| json!({"target":target,"status":"unexecuted","attempts":0,"report":null}))
        .collect();
    let executor = Executor::new(Arc::clone(interrupted));
    while !interrupted.load(Ordering::Relaxed) {
        let remaining = config
            .limits
            .campaign_ms
            .map(|budget| u128::from(budget).saturating_sub(started.elapsed().as_millis()));
        if remaining == Some(0) {
            break;
        }
        let Some(index) = choose_next(&charged, &stopped) else {
            break;
        };
        let mut limits = config.limits.clone();
        limits.campaign_ms = remaining.map(|m| u64::try_from(m).unwrap_or(u64::MAX));
        let report = executor
            .run(ExecutorConfig {
                python: config.python.clone(),
                pytest_args: config.pytest_args.clone(),
                invocations: vec![Invocation {
                    test_id: config.targets[index].clone(),
                    inputs: vec![],
                }],
                limits,
                seed: config
                    .seed
                    .wrapping_add(u64::try_from(charged[index]).unwrap_or(u64::MAX)),
                search: true,
                feedback: true,
                baseline: false,
                output: config.store.join(format!("target-{index}")),
                identity: json!({"environment":config.identity,"target":config.targets[index]}),
            })
            .map_err(io::Error::other)?;
        charged[index] = charged[index].saturating_add(report.exploration_ms.max(1));
        stopped[index] = report.outcomes.iter().any(|outcome| {
            !["pass", "reject", "campaign_cancelled"].contains(&outcome.kind.as_str())
        });
        let attempts = targets[index]["attempts"]
            .as_u64()
            .unwrap_or(0)
            .saturating_add(report.attempts);
        let previous = &targets[index];
        let mut totals = previous["totals"].clone();
        if !totals.is_object() {
            totals = json!({});
        }
        for (key, value) in [
            ("useful", report.useful),
            ("rejected", report.rejected),
            ("adapter_ns", report.adapter_ns),
            ("dispatch_roundtrip_ns", report.dispatch_roundtrip_ns),
        ] {
            totals[key] = json!(totals[key].as_u64().unwrap_or(0).saturating_add(value));
        }
        let mut output: Vec<u8> =
            serde_json::from_value(previous["retained_output"]["tail"].clone()).unwrap_or_default();
        let mut observed_output = previous["retained_output"]["observed"]
            .as_u64()
            .unwrap_or(0);
        for log in &report.logs {
            output.extend(&log.tail);
            observed_output = observed_output.saturating_add(log.observed);
        }
        let excess = output
            .len()
            .saturating_sub(config.limits.retained_output_bytes);
        output.drain(..excess);
        let retained_output = json!({"tail":output,"observed":observed_output,"truncated":observed_output > u64::try_from(output.len()).unwrap_or(u64::MAX)});
        targets[index] = json!({"target":config.targets[index],"status":if stopped[index]{"stopped"}else{"searched"},"attempts":attempts,"totals":totals,"retained_output":retained_output,"charged_ms":charged[index],"report":report});
    }
    Ok(
        json!({"schema_version":1,"command":"run","effective_limits":config.limits,"targets":targets,"environment":config.identity,"interrupted":interrupted.load(Ordering::Relaxed),"elapsed_ms":started.elapsed().as_millis()}),
    )
}
/// Map a product report to the documented process exit status.
#[must_use]
pub fn exit_code(report: &Value) -> i32 {
    if report["interrupted"] == true {
        return 130;
    }
    let mut code = 0;
    if let Some(targets) = report["targets"].as_array() {
        for target in targets {
            if let Some(outcomes) = target["report"]["outcomes"].as_array() {
                for outcome in outcomes {
                    match outcome["kind"].as_str().unwrap_or("infrastructure") {
                        "infrastructure" => return 2,
                        "campaign_cancelled" if !outcome["original"].is_null() => code = 1,
                        "pass" | "reject" | "skip" | "campaign_cancelled" => (),
                        _ => code = 1,
                    }
                }
            }
        }
    }
    code
}
