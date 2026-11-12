//! Equal-time allocation: charge actual slice wall time, including cooperative overrun.
use crate::executor::{ExecutionReport, ExecutionSettings, Executor, Limits, LogTail, OutcomeKind};
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
    least_charged(
        charged
            .iter()
            .copied()
            .enumerate()
            .filter(|(i, _)| !stopped.get(*i).copied().unwrap_or(true)),
    )
}
// Stable iteration order breaks equal-charge ties for both the typed scheduler
// and the public pure adapter used by Lean correspondence and fuzzing.
fn least_charged(entries: impl Iterator<Item = (usize, u128)>) -> Option<usize> {
    entries.min_by_key(|(_, time)| *time).map(|(i, _)| i)
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
pub(crate) enum TargetStatus {
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
    kind: OutcomeKind,
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
    /// Charge exploration debt while retaining a completed slice's evidence.
    /// Replay/shrink time never creates debt; a zero-duration slice costs one ms.
    /// Status owns scheduling eligibility, so callers cannot forget a stop flag.
    fn record(&mut self, report: ExecutionReport, output_limit: usize) {
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
        accumulated.charged_ms = accumulated
            .charged_ms
            .saturating_add(report.exploration_ms.max(1));
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
    }
    /// Unexecuted targets start with no exploration debt.
    fn charged_ms(&self) -> u128 {
        self.accumulated
            .as_ref()
            .map_or(0, |progress| progress.charged_ms)
    }
    /// Scheduling reads the same status and debt that reporting publishes.
    fn choose_next(targets: &[Self]) -> Option<usize> {
        least_charged(
            targets
                .iter()
                .enumerate()
                .filter(|(_, target)| {
                    matches!(
                        target.status,
                        TargetStatus::Unexecuted | TargetStatus::Searched
                    )
                })
                .map(|(index, target)| (index, target.charged_ms())),
        )
    }
    /// Replace the latest snapshot with an infrastructure failure while preserving
    /// counters, retained output and this target's accumulated exploration charge.
    fn record_error(&mut self, detail: String) {
        self.status = TargetStatus::Infrastructure;
        self.report = Some(SliceSnapshot::Error {
            outcomes: [InfrastructureOutcome {
                kind: OutcomeKind::Infrastructure,
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
        let Some(index) = TargetProgress::choose_next(&targets) else {
            break;
        };
        let target = &targets[index];
        let mut limits = config.limits.clone();
        limits.campaign_ms = remaining.map(|m| u64::try_from(m).unwrap_or(u64::MAX));
        let report = match executor.explore(
            &target.target,
            ExecutionSettings {
                python: config.python.clone(),
                pytest_args: config.pytest_args.clone(),
                limits,
                seed: config
                    .seed
                    .wrapping_add(u64::try_from(target.charged_ms()).unwrap_or(u64::MAX)),
                output: config.store.join(format!("target-{index}")),
                identity: json!({"environment":config.identity,"target":target.target}),
            },
        ) {
            Ok(report) => report,
            Err(error) => {
                targets[index].record_error(error.to_string());
                break;
            }
        };
        targets[index].record(report, config.limits.retained_output_bytes);
    }
    Ok(
        json!({"schema_version":1,"command":"run","effective_limits":config.limits,"targets":targets,"environment":config.identity,"interrupted":interrupted.load(Ordering::Relaxed),"elapsed_ms":started.elapsed().as_millis()}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::{ExecutionReport, LogTail};
    fn progress_after_two_slices() -> TargetProgress {
        let mut progress = TargetProgress::new("test_x".into());
        let mut first = LogTail::default();
        first.append(b"abcdefgh", 3);
        progress.record(
            ExecutionReport {
                attempts: 5,
                exploration_ms: 10,
                useful: 3,
                rejected: 2,
                logs: vec![first],
                ..ExecutionReport::default()
            },
            3,
        );
        let mut second = LogTail::default();
        second.append(b"xy", 3);
        progress.record(
            ExecutionReport {
                attempts: 2,
                exploration_ms: 11,
                useful: 1,
                rejected: 1,
                logs: vec![second],
                ..ExecutionReport::default()
            },
            3,
        );
        progress
    }
    #[test]
    fn initial_progress_is_unexecuted() {
        let progress = TargetProgress::new("test_x".into());
        assert_eq!(
            serde_json::to_value(&progress).unwrap(),
            json!({"target":"test_x","status":"unexecuted","attempts":0,"report":null})
        );
    }
    #[test]
    fn progress_accumulates_typed_counters_and_preserves_last_slice_shape() {
        let progress = progress_after_two_slices();
        let value = serde_json::to_value(&progress).unwrap();
        assert_eq!(value["attempts"], 7);
        assert_eq!(value["totals"]["useful"], 4);
        assert_eq!(value["totals"]["rejected"], 3);
        assert_eq!(value["report"]["attempts"], 2);
        assert_eq!(value["charged_ms"], 21);
        assert_eq!(value["retained_output"]["tail"], json!(b"hxy"));
        assert_eq!(value["retained_output"]["observed"], 10);
    }
    #[test]
    fn progress_error_preserves_accumulated_state() {
        let mut progress = progress_after_two_slices();
        let value = serde_json::to_value(&progress).unwrap();
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
    proptest::proptest! {
        #[test]
        fn accumulated_attempts_and_exploration_debt_saturate(first in proptest::prelude::any::<u64>(), second in proptest::prelude::any::<u64>(), first_charge in proptest::prelude::any::<u128>(), second_charge in proptest::prelude::any::<u128>()) {
            let mut progress = TargetProgress::new("test_x".into());
            progress.record(ExecutionReport { attempts: first, exploration_ms: first_charge, ..ExecutionReport::default() }, 0);
            progress.record(ExecutionReport { attempts: second, exploration_ms: second_charge, ..ExecutionReport::default() }, 0);
            proptest::prop_assert_eq!(progress.attempts, first.saturating_add(second));
            proptest::prop_assert_eq!(progress.charged_ms(), first_charge.max(1).saturating_add(second_charge.max(1)));
        }
    }
    #[rstest::rstest]
    #[case::zero(0, 1)]
    #[case::positive(19, 19)]
    #[case::maximum(u128::MAX, u128::MAX)]
    fn target_records_its_own_exploration_charge(#[case] elapsed: u128, #[case] expected: u128) {
        let mut target = TargetProgress::new("test_x".into());
        target.record(
            ExecutionReport {
                exploration_ms: elapsed,
                ..ExecutionReport::default()
            },
            0,
        );
        assert_eq!(target.charged_ms(), expected);
        assert_eq!(TargetProgress::choose_next(&[target]), Some(0));
    }
    #[rstest::rstest]
    #[case::unexecuted(TargetStatus::Unexecuted, Some(0))]
    #[case::searched(TargetStatus::Searched, Some(0))]
    #[case::stopped(TargetStatus::Stopped, None)]
    #[case::infrastructure(TargetStatus::Infrastructure, None)]
    fn target_status_owns_scheduling_eligibility(
        #[case] status: TargetStatus,
        #[case] expected: Option<usize>,
    ) {
        let mut target = TargetProgress::new("test_x".into());
        target.status = status;
        assert_eq!(TargetProgress::choose_next(&[target]), expected);
    }
    #[test]
    fn equal_unexecuted_targets_keep_stable_index_order() {
        let targets = [
            TargetProgress::new("first".into()),
            TargetProgress::new("second".into()),
        ];
        assert_eq!(TargetProgress::choose_next(&targets), Some(0));
    }
    // Construct one row of a generated schedule, not a table of unrelated cases.
    fn scheduled_target((charge, stopped): (u128, bool)) -> TargetProgress {
        TargetProgress {
            status: if stopped {
                TargetStatus::Stopped
            } else {
                TargetStatus::Searched
            },
            accumulated: Some(AccumulatedProgress {
                charged_ms: charge,
                ..AccumulatedProgress::default()
            }),
            ..TargetProgress::new("generated".into())
        }
    }
    proptest::proptest! {
        #[test]
        fn typed_schedule_agrees_with_public_oracle(entries in proptest::collection::vec((proptest::prelude::any::<u128>(), proptest::prelude::any::<bool>()), 0..40)) {
            let charged: Vec<_> = entries.iter().map(|(charge, _)| *charge).collect();
            let stopped: Vec<_> = entries.iter().map(|(_, stop)| *stop).collect();
            let targets: Vec<_> = entries.into_iter().map(scheduled_target).collect();
            proptest::prop_assert_eq!(TargetProgress::choose_next(&targets), choose_next(&charged, &stopped));
        }
        #[test]
        fn replay_and_shrink_time_do_not_create_scheduler_debt(exploration in 0_u128..1000, replay in proptest::prelude::any::<u128>(), shrink in proptest::prelude::any::<u128>()) {
            let mut target = TargetProgress::new("test_x".into());
            target.record(ExecutionReport { exploration_ms: exploration, replay_ms: replay, shrink_ms: shrink, ..ExecutionReport::default() }, 0);
            proptest::prop_assert_eq!(target.charged_ms(), exploration.max(1));
        }
    }
}
