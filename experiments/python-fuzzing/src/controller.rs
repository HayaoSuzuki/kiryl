// Copyright (c) 2026 kiryl contributors
//! One-worker probe: Rust owns input, time, persistence and bounded output.
use crate::{artifacts::ArtifactStore, resource, wire};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::{self, Read},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("configuration: {0}")]
    Configuration(String),
    #[error("transport: {0}")]
    Transport(#[from] io::Error),
    #[error("storage: {0}")]
    Storage(io::Error),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invocation {
    pub test_id: String,
    pub inputs: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub campaign_ms: Option<u64>,
    pub case_ms: u64,
    pub slice_ms: u64,
    pub tree_memory_bytes: u64,
    pub retained_output_bytes: usize,
    pub allow_best_effort: bool,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            campaign_ms: None,
            case_ms: 30_000,
            slice_ms: 60_000,
            tree_memory_bytes: 1 << 30,
            retained_output_bytes: 1 << 20,
            allow_best_effort: false,
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeConfig {
    pub python: PathBuf,
    pub pytest_args: Vec<String>,
    pub invocations: Vec<Invocation>,
    #[serde(default)]
    pub limits: Limits,
    #[serde(default)]
    pub seed: u64,
    #[serde(default)]
    pub search: bool,
    pub output: PathBuf,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Observation {
    pub kind: String,
    pub canonical: Option<Vec<u8>>,
    pub fingerprint: Option<String>,
    pub replay_token: Option<Vec<u8>>,
    pub rendered: Option<String>,
    pub features: Vec<String>,
    pub verification: String,
}
#[derive(Debug, Serialize)]
pub struct Outcome {
    pub test_id: String,
    pub case_id: u64,
    pub kind: String,
    pub original: Option<String>,
    pub derived: Vec<String>,
    pub verification: String,
}
#[derive(Debug, Default, Serialize)]
pub struct LogTail {
    pub tail: VecDeque<u8>,
    pub observed: u64,
    pub truncated: bool,
}
impl LogTail {
    pub fn append(&mut self, bytes: &[u8], limit: usize) {
        self.observed = self.observed.saturating_add(bytes.len() as u64);
        let keep = bytes.len().min(limit);
        while self.tail.len() + keep > limit {
            self.tail.pop_front();
        }
        self.tail.extend(&bytes[bytes.len() - keep..]);
        self.truncated = self.observed > self.tail.len() as u64;
    }
}
#[derive(Debug, Default, Serialize)]
pub struct ProbeReport {
    pub version: u8,
    pub outcomes: Vec<Outcome>,
    pub logs: Vec<LogTail>,
    pub attempts: u64,
    pub useful: u64,
    pub rejected: u64,
    pub interrupted: bool,
    pub elapsed_ms: u128,
    pub capabilities: Value,
}
impl ProbeReport {
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        if self.interrupted {
            130
        } else if self.outcomes.iter().any(|o| o.kind == "infrastructure") {
            2
        } else {
            i32::from(
                self.outcomes
                    .iter()
                    .any(|o| o.kind != "pass" && o.kind != "reject"),
            )
        }
    }
}

/// Deterministic budget precedence shared by live supervision and model tests.
#[must_use]
pub fn expiry(interrupted: bool, campaign: bool, case: bool) -> Option<&'static str> {
    if interrupted {
        Some("interrupted")
    } else if campaign {
        Some("campaign_cancelled")
    } else if case {
        Some("case_timeout")
    } else {
        None
    }
}

struct Worker {
    child: Child,
    input: ChildStdin,
    events: Receiver<io::Result<Value>>,
    log: Arc<Mutex<LogTail>>,
    log_done: Receiver<()>,
    resource: resource::Guard,
    cleanup_error: Option<String>,
}
impl Worker {
    fn launch(config: &ProbeConfig) -> io::Result<Self> {
        let mut command = Command::new(&config.python);
        command
            .args([
                "-I",
                "-S",
                "-c",
                "import sys; sys.path.insert(0, sys.argv[1]); import worker; worker.main()",
            ])
            .arg(env!("CARGO_MANIFEST_DIR"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("PYTHONUNBUFFERED", "1");
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn()?;
        let resource =
            match resource::attach(&mut child, &config.limits, config.limits.allow_best_effort) {
                Ok(guard) => guard,
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error);
                }
            };
        let input = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("missing stdin"))?;
        let mut output = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("missing stdout"))?;
        let mut errors = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("missing stderr"))?;
        let (send, events) = mpsc::sync_channel(4);
        thread::spawn(move || {
            loop {
                match wire::read(&mut output) {
                    Ok(Some(value)) => {
                        if send.send(Ok(value)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        let _ = send.send(Err(error));
                        break;
                    }
                }
            }
        });
        let log = Arc::new(Mutex::new(LogTail::default()));
        let sink = Arc::clone(&log);
        let limit = config.limits.retained_output_bytes;
        let (log_finished, log_done) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut buffer = [0; 8192];
            while let Ok(size) = errors.read(&mut buffer) {
                if size == 0 {
                    break;
                }
                if let Ok(mut tail) = sink.lock() {
                    tail.append(&buffer[..size], limit);
                }
            }
            let _ = log_finished.send(());
        });
        Ok(Self {
            child,
            input,
            events,
            log,
            log_done,
            resource,
            cleanup_error: None,
        })
    }
    fn send(&mut self, value: Value) -> io::Result<()> {
        wire::write(&mut self.input, value)
    }
    fn terminate(&mut self) {
        if let Err(error) = self.resource.terminate_tree() {
            self.cleanup_error = Some(error.to_string());
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = self.log_done.recv_timeout(Duration::from_millis(100));
    }
    fn take_log(&self) -> LogTail {
        self.log
            .lock()
            .map(|mut l| std::mem::take(&mut *l))
            .unwrap_or_default()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.terminate();
    }
}

struct RunResult {
    observation: Option<Observation>,
    original: Option<String>,
    consumed: usize,
    derived: Vec<Value>,
}

pub struct Controller {
    interrupted: Arc<AtomicBool>,
}
impl Controller {
    #[must_use]
    pub fn new(interrupted: Arc<AtomicBool>) -> Self {
        Self { interrupted }
    }
    /// Execute the finite experimental schedule, preserving findings before replay.
    /// # Errors
    /// Returns invalid configuration, transport setup, or durable write failures.
    #[allow(clippy::needless_pass_by_value, clippy::too_many_lines)] // owned public configuration is the planned boundary
    pub fn run(&self, config: ProbeConfig) -> Result<ProbeReport, ProbeError> {
        if config.search
            || config.limits.case_ms == 0
            || config.limits.slice_ms == 0
            || config.limits.retained_output_bytes > 16 * 1024 * 1024
        {
            return Err(ProbeError::Configuration(
                "invalid/unsupported probe settings".into(),
            ));
        }
        let mut store = ArtifactStore::new(&config.output).map_err(ProbeError::Storage)?;
        let start = Instant::now();
        let mut report = ProbeReport {
            version: 1,
            capabilities: serde_json::to_value(resource::detect()).map_err(io::Error::from)?,
            ..ProbeReport::default()
        };
        if !config.limits.allow_best_effort && report.capabilities["tree_memory_hard"] != true {
            report.outcomes.push(Outcome {
                test_id: String::new(),
                case_id: 0,
                kind: "infrastructure".into(),
                original: None,
                derived: vec![],
                verification: "hard memory backend unavailable".into(),
            });
            return Ok(report);
        }
        for invocation in &config.invocations {
            let mut offset = 0;
            while offset < invocation.inputs.len() {
                let result = self.session(
                    &config,
                    invocation,
                    offset,
                    "run",
                    None,
                    start,
                    &mut store,
                    &mut report,
                )?;
                offset += result.consumed;
                if let Some(observed) = result.observation {
                    let index = report
                        .outcomes
                        .iter()
                        .rposition(|o| o.original == result.original && o.kind == observed.kind)
                        .unwrap_or_else(|| report.outcomes.len().saturating_sub(1));
                    if observed.kind == "failure"
                        && let (Some(token), Some(original)) =
                            (&observed.replay_token, &result.original)
                        && !self.cancelled(&config, start)
                    {
                        let verified = self.session(
                            &config,
                            invocation,
                            0,
                            "replay",
                            Some(token),
                            start,
                            &mut store,
                            &mut report,
                        )?;
                        let verification = verified
                            .observation
                            .as_ref()
                            .map_or("pending", |o| o.verification.as_str())
                            .to_owned();
                        report.outcomes[index]
                            .verification
                            .clone_from(&verification);
                        if let Some(value) = verified.observation {
                            let path = store
                                .derived(original, &json!({"mode": "replay", "observation": value}))
                                .map_err(ProbeError::Storage)?;
                            report.outcomes[index].derived.push(path);
                        }
                        if verification == "confirmed" && !self.cancelled(&config, start) {
                            let shrunk = self.session(
                                &config,
                                invocation,
                                0,
                                "minimize",
                                Some(token),
                                start,
                                &mut store,
                                &mut report,
                            )?;
                            for evidence in shrunk.derived {
                                report.outcomes[index].derived.push(
                                    store
                                        .derived(original, &evidence)
                                        .map_err(ProbeError::Storage)?,
                                );
                            }
                        }
                    }
                    if ["native_crash", "case_timeout"].contains(&observed.kind.as_str())
                        && let Some(original) = &result.original
                        && !self.cancelled(&config, start)
                    {
                        let verification = self.session(
                            &config,
                            invocation,
                            offset.saturating_sub(1),
                            "raw_replay",
                            None,
                            start,
                            &mut store,
                            &mut report,
                        )?;
                        let status = if verification.observation.as_ref().is_none_or(|v| {
                            ["campaign_cancelled", "interrupted", "infrastructure"]
                                .contains(&v.kind.as_str())
                        }) {
                            "pending"
                        } else if verification.observation.as_ref().is_some_and(|v| {
                            v.kind == observed.kind
                                && (v.kind != "native_crash" || v.rendered == observed.rendered)
                        }) {
                            "confirmed"
                        } else {
                            "unstable"
                        };
                        report.outcomes[index].verification = status.into();
                        let evidence = json!({"mode": "raw_replay", "verification": status, "observation": verification.observation});
                        report.outcomes[index].derived.push(
                            store
                                .derived(original, &evidence)
                                .map_err(ProbeError::Storage)?,
                        );
                    }
                    break;
                }
                if self.cancelled(&config, start) || result.consumed == 0 {
                    break;
                }
            }
            if self.cancelled(&config, start) {
                break;
            }
        }
        report.interrupted = self.interrupted.load(Ordering::Relaxed);
        report.elapsed_ms = start.elapsed().as_millis();
        Ok(report)
    }
    fn cancelled(&self, config: &ProbeConfig, start: Instant) -> bool {
        self.interrupted.load(Ordering::Relaxed)
            || config
                .limits
                .campaign_ms
                .is_some_and(|m| start.elapsed() >= Duration::from_millis(m))
    }
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)] // one transport state machine, reviewed as a unit
    fn session(
        &self,
        config: &ProbeConfig,
        invocation: &Invocation,
        offset: usize,
        mode: &str,
        token: Option<&Vec<u8>>,
        campaign_start: Instant,
        store: &mut ArtifactStore,
        report: &mut ProbeReport,
    ) -> Result<RunResult, ProbeError> {
        let mut worker = match Worker::launch(config) {
            Ok(worker) => worker,
            Err(error) => {
                let observation = infrastructure(&format!("worker/resource setup: {error}"));
                report.outcomes.push(Outcome {
                    test_id: invocation.test_id.clone(),
                    case_id: 0,
                    kind: observation.kind.clone(),
                    original: None,
                    derived: vec![],
                    verification: "pending".into(),
                });
                return Ok(RunResult {
                    observation: Some(observation),
                    original: None,
                    consumed: 0,
                    derived: vec![],
                });
            }
        };
        report.capabilities =
            serde_json::to_value(worker.resource.capabilities()).map_err(io::Error::from)?;
        let mut consumed = 0;
        let mut active: Option<(u64, Vec<u8>, Instant)> = None;
        let mut observed = None;
        let mut original = None;
        let mut derived = vec![];
        let mut slice_start = None;

        let mut ready = false;
        let mut closed_at = None;
        let mut resource_check = Instant::now();
        loop {
            let interruption = self.interrupted.load(Ordering::Relaxed);
            let campaign_expired = config
                .limits
                .campaign_ms
                .is_some_and(|m| campaign_start.elapsed() >= Duration::from_millis(m));
            let case_expired = active.as_ref().is_some_and(|(_, _, start)| {
                start.elapsed() >= Duration::from_millis(config.limits.case_ms)
            });
            if let Some(reason) = expiry(interruption, campaign_expired, case_expired) {
                observed = Some(Observation {
                    kind: reason.into(),
                    verification: "pending".into(),
                    ..Observation::default()
                });
                break;
            }
            if active.is_some() && resource_check.elapsed() >= Duration::from_millis(20) {
                resource_check = Instant::now();
                match worker.resource.oom_observed() {
                    Ok(true) => {
                        observed = Some(Observation {
                            kind: "oom".into(),
                            verification: "pending".into(),
                            ..Observation::default()
                        });
                        break;
                    }
                    Ok(false) => (),
                    Err(error) => {
                        observed = Some(infrastructure(&format!("resource evidence: {error}")));
                        break;
                    }
                }
            }
            match worker.events.recv_timeout(Duration::from_millis(2)) {
                Ok(Ok(message)) => {
                    let kind = message["kind"].as_str().unwrap_or("");
                    match kind {
                        "ready" if !ready => {
                            ready = true;
                            let budget = config.limits.campaign_ms.map_or(1.0, |m| {
                                (Duration::from_millis(m).saturating_sub(campaign_start.elapsed()))
                                    .as_secs_f64()
                            });
                            worker.send(json!({"kind": "start", "pytest_args": config.pytest_args, "test_id": invocation.test_id,
                                "mode": if mode == "raw_replay" { "run" } else { mode }, "token": token, "budget_seconds": budget}))?;
                            if !["run", "raw_replay"].contains(&mode) {
                                active = Some((
                                    0,
                                    invocation.inputs.first().cloned().unwrap_or_default(),
                                    Instant::now(),
                                ));
                            }
                        }
                        "next"
                            if ready
                                && ["run", "raw_replay"].contains(&mode)
                                && active.is_none() =>
                        {
                            if message["test_id"].as_str() != Some(&invocation.test_id) {
                                observed = Some(infrastructure("test ID mismatch"));
                                break;
                            }
                            if slice_start.get_or_insert_with(Instant::now).elapsed()
                                >= Duration::from_millis(config.limits.slice_ms)
                                || observed.is_some()
                                || offset + consumed >= invocation.inputs.len()
                                || (mode == "raw_replay" && consumed >= 1)
                            {
                                worker.send(json!({"kind": "end"}))?;
                            } else {
                                let input = invocation.inputs[offset + consumed].clone();
                                if mode == "run" {
                                    report.attempts += 1;
                                }
                                let id = if mode == "run" { report.attempts } else { 0 };
                                active = Some((id, input.clone(), Instant::now()));
                                consumed += 1;
                                worker
                                    .send(json!({"kind": "case", "case_id": id, "input": input}))?;
                            }
                        }
                        "result" if ready && mode != "minimize" => {
                            let Some((id, input, _)) = active.as_ref() else {
                                observed = Some(infrastructure("result without active case"));
                                break;
                            };
                            if message["case_id"].as_u64() != Some(*id)
                                || message["observation"]["nodeid"].as_str()
                                    != Some(&invocation.test_id)
                            {
                                observed = Some(infrastructure("case ID mismatch"));
                                break;
                            }
                            let Ok(value): Result<Observation, _> =
                                serde_json::from_value(message["observation"].clone())
                            else {
                                observed = Some(infrastructure("malformed observation"));
                                break;
                            };
                            let mut value = value;
                            if worker.resource.oom_observed()? {
                                value.kind = "oom".into();
                            }
                            if !["pass", "reject", "failure", "oom"].contains(&value.kind.as_str())
                            {
                                observed = Some(infrastructure("unknown observation"));
                                break;
                            }
                            if mode == "run" {
                                if value.kind == "reject" {
                                    report.rejected += 1;
                                } else {
                                    report.useful += 1;
                                }
                                if ["failure", "oom"].contains(&value.kind.as_str()) {
                                    original = Some(
                                        store
                                            .record_original(&invocation.test_id, input, &value)
                                            .map_err(ProbeError::Storage)?,
                                    );
                                    observed = Some(value.clone());
                                }
                                if report.outcomes.len() < 256
                                    || ["failure", "oom"].contains(&value.kind.as_str())
                                {
                                    report.outcomes.push(Outcome {
                                        test_id: invocation.test_id.clone(),
                                        case_id: *id,
                                        kind: value.kind.clone(),
                                        original: original.clone(),
                                        derived: vec![],
                                        verification: "pending".into(),
                                    });
                                }
                            } else {
                                observed = Some(value);
                            }
                            active = None;
                            worker.send(json!({"kind": "ack"}))?;
                        }
                        "candidate" if ready && mode == "minimize" => {
                            let candidate: Vec<u8> =
                                serde_json::from_value(message["token"].clone())
                                    .map_err(io::Error::from)?;
                            // Retain the entire opaque token before authorizing its execution.
                            active = Some((0, candidate, Instant::now()));
                            worker.send(json!({"kind": "ack"}))?;
                        }
                        "shrunk" if ready && mode == "minimize" => {
                            // Caller owns original lineage; return a separate record for this attempt.
                            derived.push(json!({"mode": "minimize", "token": message["token"], "complete": message["complete"], "verification": "pending"}));
                            active = None;
                        }
                        "done" if ready => {
                            break;
                        }
                        "error" => {
                            observed = Some(infrastructure(
                                message["message"].as_str().unwrap_or("worker error"),
                            ));
                            break;
                        }
                        _ => {
                            observed = Some(infrastructure("unexpected control frame"));
                            break;
                        }
                    }
                }
                Ok(Err(error)) => {
                    observed = Some(infrastructure(&error.to_string()));
                    break;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let status = worker.child.try_wait()?;
                    if status.is_none()
                        && closed_at.get_or_insert_with(Instant::now).elapsed()
                            < Duration::from_millis(20)
                    {
                        thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    if observed.is_none() && worker.resource.oom_observed()? {
                        observed = Some(Observation {
                            kind: if active.is_some() {
                                "oom"
                            } else {
                                "infrastructure"
                            }
                            .into(),
                            verification: "pending".into(),
                            ..Observation::default()
                        });
                    }
                    if observed.is_none() {
                        observed =
                            Some(if active.is_some() && status.is_some_and(native_failure) {
                                Observation {
                                    kind: "native_crash".into(),
                                    rendered: status.map(|s| s.to_string()),
                                    verification: "pending".into(),
                                    ..Observation::default()
                                }
                            } else {
                                infrastructure("worker exited before done")
                            });
                    }
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => (),
            }
        }
        worker.terminate();
        report.logs.push(worker.take_log());
        if let Some(error) = &worker.cleanup_error {
            report.outcomes.push(Outcome {
                test_id: invocation.test_id.clone(),
                case_id: 0,
                kind: "infrastructure".into(),
                original: None,
                derived: vec![],
                verification: format!("cleanup failed: {error}"),
            });
        }
        if let Some(value) = &observed {
            let failure = !(["pass", "reject", "failure"].contains(&value.kind.as_str())
                || value.kind == "oom" && original.is_some());
            if mode == "minimize"
                && let Some((_, candidate, _)) = &active
            {
                derived
                    .push(json!({"mode": "minimize", "active_token": candidate, "outcome": value}));
            }
            if failure && mode == "run" {
                let id = active.as_ref().map_or(0, |(id, _, _)| *id);
                if let Some((_, input, _)) = active {
                    original = Some(
                        store
                            .record_original(&invocation.test_id, &input, value)
                            .map_err(ProbeError::Storage)?,
                    );
                }
                report.outcomes.push(Outcome {
                    test_id: invocation.test_id.clone(),
                    case_id: id,
                    kind: value.kind.clone(),
                    original: original.clone(),
                    derived: vec![],
                    verification: "pending".into(),
                });
            }
        }
        Ok(RunResult {
            observation: observed,
            original,
            consumed,
            derived,
        })
    }
}
#[cfg(any(windows, test))]
fn windows_abnormal_exit(code: i32) -> bool {
    // CRT abort is exit 3 after bootstrap disables dialogs/report-fault. This
    // candidate is inherently indistinguishable from an explicit exit(3).
    code == 3 || code < 0
}
fn native_failure(status: std::process::ExitStatus) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.signal().is_some()
    }
    #[cfg(windows)]
    {
        status.code().is_some_and(windows_abnormal_exit)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = status;
        false
    }
}
fn infrastructure(message: &str) -> Observation {
    Observation {
        kind: "infrastructure".into(),
        rendered: Some(message.into()),
        verification: "pending".into(),
        ..Observation::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    proptest! {
        #[test]
        fn tail_preserves_exact_suffix(chunks in proptest::collection::vec(proptest::collection::vec(any::<u8>(), 0..256), 0..32), limit in 0usize..1024) {
            let mut tail = LogTail::default();
            let mut all: Vec<u8> = Vec::new();
            for chunk in chunks { all.extend(&chunk); tail.append(&chunk, limit); }
            let start = all.len().saturating_sub(limit);
            prop_assert_eq!(tail.tail.into_iter().collect::<Vec<_>>(), &all[start..]);
            prop_assert_eq!(tail.observed, all.len() as u64);
        }
    }
    #[test]
    fn lean_generated_budget_correspondence() {
        for line in include_str!("../formal/budget-cases.tsv").lines() {
            let fields: Vec<_> = line.split('\t').collect();
            let expected = expiry(
                fields[0] == "true",
                fields[1] == "true",
                fields[2] == "true",
            )
            .unwrap_or("running");
            assert_eq!(expected, fields[3]);
        }
    }
    #[test]
    fn windows_abnormal_exit_table_is_candidate_evidence_only() {
        assert!(windows_abnormal_exit(3));
        assert!(windows_abnormal_exit(-1_073_741_819));
        assert!(!windows_abnormal_exit(7));
        assert!(!windows_abnormal_exit(0));
    }
    #[test]
    fn precedence() {
        assert_eq!(expiry(false, true, true), Some("campaign_cancelled"));
        assert_eq!(expiry(true, true, true), Some("interrupted"));
        assert_eq!(expiry(false, false, true), Some("case_timeout"));
    }
}
