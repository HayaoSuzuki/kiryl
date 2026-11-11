//! Equal-time allocation: charge actual slice wall time, including cooperative overrun.
use crate::executor::{ExecutionReport, ExecutionSettings, Executor, Limits, LogTail};
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
/// Charges without a corresponding target status are ineligible.
#[must_use]
pub fn choose_next(charged: &[u128], stopped: &[bool]) -> Option<usize> {
    charged
        .iter()
        .enumerate()
        .filter(|(i, _)| !stopped.get(*i).copied().unwrap_or(true))
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
/// One target's cumulative statistics and latest bounded slice snapshot.
/// Error transitions replace only the snapshot; prior statistics remain available.
/// Serialization preserves the public report shape without using JSON as state.
#[derive(serde::Serialize)]
struct TargetProgress {
    target: String,
    status: TargetStatus,
    attempts: u64,
    #[serde(flatten)]
    accumulated: Option<AccumulatedProgress>,
    report: Option<SliceSnapshot>,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum TargetStatus {
    Unexecuted,
    Searched,
    Stopped,
    Infrastructure,
}
#[derive(Default, serde::Serialize)]
struct AccumulatedProgress {
    totals: Counters,
    retained_output: LogTail,
    charged_ms: u128,
}
#[derive(Default, serde::Serialize)]
struct Counters {
    useful: u64,
    rejected: u64,
    adapter_ns: u64,
    dispatch_roundtrip_ns: u64,
}
#[derive(serde::Serialize)]
#[serde(untagged)]
enum SliceSnapshot {
    Execution(Box<ExecutionReport>),
    Error {
        outcomes: [InfrastructureOutcome; 1],
    },
}
#[derive(serde::Serialize)]
struct InfrastructureOutcome {
    kind: &'static str,
    detail: String,
}
impl TargetProgress {
    fn new(target: String) -> Self {
        Self {
            target,
            status: TargetStatus::Unexecuted,
            attempts: 0,
            accumulated: None,
            report: None,
        }
    }
    /// Accumulate a completed slice and replace its predecessor's snapshot.
    /// The scheduler supplies its cumulative charge, not a delta to add again.
    /// Returns true when this target must not receive another exploration slice.
    fn record(
        &mut self,
        report: ExecutionReport,
        cumulative_charged_ms: u128,
        output_limit: usize,
    ) -> bool {
        let stopped = report
            .outcomes
            .iter()
            .any(|outcome| crate::report::stops_search(&outcome.kind));
        self.status = if stopped {
            TargetStatus::Stopped
        } else {
            TargetStatus::Searched
        };
        self.attempts = self.attempts.saturating_add(report.attempts);
        let accumulated = self
            .accumulated
            .get_or_insert_with(AccumulatedProgress::default);
        accumulated.charged_ms = cumulative_charged_ms;
        accumulated.totals.useful = accumulated.totals.useful.saturating_add(report.useful);
        accumulated.totals.rejected = accumulated.totals.rejected.saturating_add(report.rejected);
        accumulated.totals.adapter_ns = accumulated
            .totals
            .adapter_ns
            .saturating_add(report.adapter_ns);
        accumulated.totals.dispatch_roundtrip_ns = accumulated
            .totals
            .dispatch_roundtrip_ns
            .saturating_add(report.dispatch_roundtrip_ns);
        for log in &report.logs {
            accumulated.retained_output.merge(log, output_limit);
        }
        self.report = Some(SliceSnapshot::Execution(Box::new(report)));
        stopped
    }
    /// Replace the latest snapshot with an infrastructure failure while preserving
    /// counters, retained output and the scheduler's last accumulated charge.
    fn record_error(&mut self, detail: String) {
        self.status = TargetStatus::Infrastructure;
        self.report = Some(SliceSnapshot::Error {
            outcomes: [InfrastructureOutcome {
                kind: "infrastructure",
                detail,
            }],
        });
    }
}
/// Run one target at a time until all stop, interruption, or the shared budget ends.
/// # Errors
/// Returns worker or storage failures without deleting existing evidence.
pub fn run(config: &CampaignConfig, interrupted: &Arc<AtomicBool>) -> io::Result<Value> {
    let _campaign_lock = crate::artifacts::ArtifactStore::open(&config.store)?;
    let started = Instant::now();
    let mut charged = vec![0; config.targets.len()];
    let mut stopped = vec![false; config.targets.len()];
    let mut targets: Vec<_> = config
        .targets
        .iter()
        .cloned()
        .map(TargetProgress::new)
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
        let report = match executor.explore(
            &config.targets[index],
            ExecutionSettings {
                python: config.python.clone(),
                pytest_args: config.pytest_args.clone(),
                limits,
                seed: config
                    .seed
                    .wrapping_add(u64::try_from(charged[index]).unwrap_or(u64::MAX)),
                output: config.store.join(format!("target-{index}")),
                identity: json!({"environment":config.identity,"target":config.targets[index]}),
            },
        ) {
            Ok(report) => report,
            Err(error) => {
                targets[index].record_error(error.to_string());
                break;
            }
        };
        charged[index] = charged[index].saturating_add(report.exploration_ms.max(1));
        stopped[index] =
            targets[index].record(report, charged[index], config.limits.retained_output_bytes);
    }
    Ok(
        json!({"schema_version":1,"command":"run","effective_limits":config.limits,"targets":targets,"environment":config.identity,"interrupted":interrupted.load(Ordering::Relaxed),"elapsed_ms":started.elapsed().as_millis()}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::{ExecutionReport, LogTail};
    #[test]
    fn progress_accumulates_typed_counters_and_preserves_last_slice_shape() {
        let mut progress = TargetProgress::new("test_x".into());
        assert_eq!(
            serde_json::to_value(&progress).unwrap(),
            json!({"target":"test_x","status":"unexecuted","attempts":0,"report":null})
        );
        let mut first = LogTail::default();
        first.append(b"abcdefgh", 3);
        progress.record(
            ExecutionReport {
                attempts: 5,
                useful: 3,
                rejected: 2,
                logs: vec![first],
                ..ExecutionReport::default()
            },
            10,
            3,
        );
        let mut second = LogTail::default();
        second.append(b"xy", 3);
        progress.record(
            ExecutionReport {
                attempts: 2,
                useful: 1,
                rejected: 1,
                logs: vec![second],
                ..ExecutionReport::default()
            },
            21,
            3,
        );
        let value = serde_json::to_value(&progress).unwrap();
        assert_eq!(value["attempts"], 7);
        assert_eq!(value["totals"]["useful"], 4);
        assert_eq!(value["totals"]["rejected"], 3);
        assert_eq!(value["report"]["attempts"], 2);
        assert_eq!(value["charged_ms"], 21);
        assert_eq!(value["retained_output"]["tail"], json!(b"hxy"));
        assert_eq!(value["retained_output"]["observed"], 10);
        progress.record_error("disk full".into());
        let failed = serde_json::to_value(progress).unwrap();
        assert_eq!(failed["totals"], value["totals"]);
        assert_eq!(failed["retained_output"], value["retained_output"]);
        assert_eq!(failed["charged_ms"], value["charged_ms"]);
        assert_eq!(failed["attempts"], 7);
        assert_eq!(
            failed["report"],
            json!({"outcomes":[{"kind":"infrastructure","detail":"disk full"}]})
        );
    }
}
