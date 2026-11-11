// Copyright (c) 2026 kiryl contributors
//! Native single-worker execution with bounded transport, deadlines and retained active inputs.
//! Mechanisms adapted from the separately tested feasibility controller at faffb19.
use crate::{
    artifacts::ArtifactStore,
    resource,
    search::Search,
    verification::{FailureIdentity, Verdict},
    wire,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::{self, Read},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, thiserror::Error)]
pub enum ExecutionError {
    #[error("configuration: {0}")]
    Configuration(String),
    #[error("transport: {0}")]
    Transport(#[from] io::Error),
    #[error("storage: {0}")]
    Storage(io::Error),
}

#[derive(Debug)]
struct Invocation {
    test_id: String,
    raw_input: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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
/// Interpreter, budgets and artifact identity for a product execution.
#[derive(Clone, Debug)]
pub(crate) struct ExecutionSettings {
    pub python: PathBuf,
    pub pytest_args: Vec<String>,
    pub limits: Limits,
    pub seed: u64,
    pub output: PathBuf,
    pub identity: Value,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Reproduction {
    Replay,
    Minimize,
}
impl Reproduction {
    fn wire_mode(self) -> &'static str {
        match self {
            Self::Replay => "replay",
            Self::Minimize => "minimize",
        }
    }
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
    pub elapsed_ns: u64,
}
#[derive(Debug, Serialize)]
pub struct Outcome {
    pub test_id: String,
    pub case_id: u64,
    pub kind: String,
    pub original: Option<String>,
    pub derived: Vec<String>,
    pub verification: String,
    pub detail: Option<String>,
}
#[derive(Debug, Default, Serialize)]
pub struct LogTail {
    tail: VecDeque<u8>,
    observed: u64,
    truncated: bool,
}
impl LogTail {
    /// Merge retained bytes while counting the source's full observed stream.
    /// A truncated source breaks continuity: discard older retained bytes rather
    /// than splice across missing history. The result is a known stream suffix.
    pub fn merge(&mut self, other: &Self, limit: usize) {
        let observed = self.observed.saturating_add(other.observed);
        if other.truncated {
            self.tail.clear();
        }
        self.append(&other.tail.iter().copied().collect::<Vec<_>>(), limit);
        self.observed = observed;
        self.truncated = observed > self.tail.len() as u64;
    }
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
pub struct ExecutionReport {
    pub version: u8,
    pub outcomes: Vec<Outcome>,
    pub logs: Vec<LogTail>,
    pub attempts: u64,
    pub useful: u64,
    pub rejected: u64,
    pub interrupted: bool,
    pub elapsed_ms: u128,
    pub capabilities: Value,
    pub environment: Value,
    pub mode: String,
    pub seed: u64,
    pub branches: usize,
    pub corpus_entries: usize,
    pub first_discovery_ms: Option<u128>,
    pub adapter_ns: u64,
    pub dispatch_roundtrip_ns: u64,
    pub replay_ms: u128,
    pub exploration_ms: u128,
    pub shrink_ms: u128,
    pub baseline_statistics: Value,
    pub operation_observation: Option<Observation>,
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
    input: Option<mpsc::SyncSender<Value>>,
    events: Receiver<io::Result<Option<Value>>>,
    send_error: Option<String>,
    log: Arc<Mutex<LogTail>>,
    log_done: Receiver<()>,
    resource: resource::Guard,
    cleanup_error: Option<String>,
}
impl Worker {
    fn launch(config: &ExecutionSettings) -> io::Result<Self> {
        let mut command = Command::new(&config.python);
        command
            .args([
                "-I",
                "-S",
                "-u",
                "-c",
                "import sys; sys.path.insert(0, sys.argv[1]); import worker; worker.main()",
            ])
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/python/kiryl_adapter"))
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
        let mut input = child
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
        let writer_events = send.clone();
        let (commands, pending) = mpsc::sync_channel(2);
        thread::spawn(move || {
            while let Ok(value) = pending.recv() {
                if let Err(error) = wire::write(&mut input, value) {
                    let _ = writer_events.send(Err(error));
                    break;
                }
            }
        });
        thread::spawn(move || {
            loop {
                match wire::read(&mut output) {
                    Ok(Some(value)) => {
                        if send.send(Ok(Some(value))).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
                        let _ = send.send(Ok(None));
                        break;
                    }
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
            input: Some(commands),
            events,
            send_error: None,
            log,
            log_done,
            resource,
            cleanup_error: None,
        })
    }
    fn enqueue(&mut self, value: Value) {
        let result = self
            .input
            .as_ref()
            .ok_or_else(|| "closed command channel".to_owned())
            .and_then(|channel| channel.try_send(value).map_err(|e| e.to_string()));
        if let Err(error) = result {
            self.send_error = Some(error);
        }
    }
    fn terminate(&mut self) {
        self.input.take();
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
    retained_input: Option<Vec<u8>>,
    derived: Vec<Value>,
}

pub(crate) struct Executor {
    interrupted: Arc<AtomicBool>,
}
impl Executor {
    #[must_use]
    pub fn new(interrupted: Arc<AtomicBool>) -> Self {
        Self { interrupted }
    }
    /// Execute one product exploration slice, preserving findings before fresh replay.
    /// Active inputs survive worker termination; unfinished checks remain pending.
    /// Cooperative slice expiry stops admission and returns through pytest teardown.
    /// # Errors
    /// Returns invalid configuration, transport setup, or durable write failures.
    #[allow(clippy::needless_pass_by_value, clippy::too_many_lines)] // owned public configuration is the planned boundary
    pub fn explore(
        &self,
        target: &str,
        config: ExecutionSettings,
    ) -> Result<ExecutionReport, ExecutionError> {
        if config.limits.tree_memory_bytes == 0
            || config.limits.case_ms == 0
            || config.limits.slice_ms == 0
            || config.limits.retained_output_bytes > 16 * 1024 * 1024
        {
            return Err(ExecutionError::Configuration(
                "invalid executor settings".into(),
            ));
        }
        let mut store = ArtifactStore::open_matching(&config.output, &config.identity)
            .map_err(ExecutionError::Storage)?;
        let start = Instant::now();
        let mut report = ExecutionReport {
            version: 1,
            mode: "guided".into(),
            seed: config.seed,
            capabilities: serde_json::to_value(resource::detect()).map_err(io::Error::from)?,
            ..ExecutionReport::default()
        };
        if !config.limits.allow_best_effort && report.capabilities["tree_memory_hard"] != true {
            report.outcomes.push(Outcome {
                test_id: String::new(),
                case_id: 0,
                kind: "infrastructure".into(),
                original: None,
                derived: vec![],
                verification: "hard memory backend unavailable".into(),
                detail: None,
            });
            return Ok(report);
        }
        let invocation = &Invocation {
            test_id: target.into(),
            raw_input: None,
        };
        let mut engine = Search::new(config.seed);
        engine.restore(store.load_corpus().map_err(ExecutionError::Storage)?);
        let mut search = Some(engine);
        let result = self.session(
            &config,
            invocation,
            "run",
            None,
            start,
            &mut store,
            &mut report,
            &mut search,
        )?;
        if let Some(observed) = result.observation {
            let index = report
                .outcomes
                .iter()
                .rposition(|o| o.original == result.original && o.kind == observed.kind)
                .unwrap_or_else(|| report.outcomes.len().saturating_sub(1));
            if observed.kind == "failure"
                && let (Some(token), Some(original)) = (&observed.replay_token, &result.original)
                && !self.cancelled(&config, start)
            {
                let replay_start = Instant::now();
                let verified = self.session(
                    &config,
                    invocation,
                    "replay",
                    Some(token),
                    start,
                    &mut store,
                    &mut report,
                    &mut None,
                )?;
                report.replay_ms += replay_start.elapsed().as_millis();
                let verification =
                    FailureIdentity::observed(&observed).check(verified.observation.as_ref());
                report.outcomes[index].verification = verification.candidate_status().into();
                if let Some(value) = verified.observation {
                    let path = store
                        .derived(original, &json!({"mode": "replay", "observation": value}))
                        .map_err(ExecutionError::Storage)?;
                    report.outcomes[index].derived.push(path);
                }
                if verification == Verdict::Confirmed && !self.cancelled(&config, start) {
                    let shrink_start = Instant::now();
                    let shrunk = self.session(
                        &config,
                        invocation,
                        "minimize",
                        Some(token),
                        start,
                        &mut store,
                        &mut report,
                        &mut None,
                    )?;
                    report.shrink_ms += shrink_start.elapsed().as_millis();
                    for mut evidence in shrunk.derived {
                        let pending = store
                            .derived(original, &evidence)
                            .map_err(ExecutionError::Storage)?;
                        report.outcomes[index].derived.push(pending);
                        if evidence.get("token").is_some() && !self.cancelled(&config, start) {
                            let candidate: Vec<u8> =
                                serde_json::from_value(evidence["token"].clone())
                                    .map_err(io::Error::from)?;
                            let checked = self.session(
                                &config,
                                invocation,
                                "replay",
                                Some(&candidate),
                                start,
                                &mut store,
                                &mut report,
                                &mut None,
                            )?;
                            evidence["verification"] = json!(
                                FailureIdentity::observed(&observed)
                                    .check(checked.observation.as_ref())
                                    .candidate_status()
                            );
                        }
                        report.outcomes[index].derived.push(
                            store
                                .derived(original, &evidence)
                                .map_err(ExecutionError::Storage)?,
                        );
                    }
                }
            }
            if ["native_crash", "case_timeout"].contains(&observed.kind.as_str())
                && let Some(original) = &result.original
                && !self.cancelled(&config, start)
            {
                let replay_invocation = Invocation {
                    test_id: invocation.test_id.clone(),
                    raw_input: result.retained_input.clone(),
                };
                let verification = self.session(
                    &config,
                    &replay_invocation,
                    "raw_replay",
                    None,
                    start,
                    &mut store,
                    &mut report,
                    &mut None,
                )?;
                let status = FailureIdentity::observed(&observed)
                    .check(verification.observation.as_ref())
                    .candidate_status();
                report.outcomes[index].verification = status.into();
                let evidence = json!({"mode": "raw_replay", "input": result.retained_input, "verification": status, "observation": verification.observation});
                report.outcomes[index].derived.push(
                    store
                        .derived(original, &evidence)
                        .map_err(ExecutionError::Storage)?,
                );
            }
        }
        if let Some(search) = search {
            store
                .save_corpus(search.corpus())
                .map_err(ExecutionError::Storage)?;
            report.branches = search.feature_count();
            report.corpus_entries = search.corpus_len();
        }
        report.interrupted = self.interrupted.load(Ordering::Relaxed);
        report.elapsed_ms = start.elapsed().as_millis();
        Ok(report)
    }
    /// Execute a fresh standalone replay or minimization using the same supervision.
    /// # Errors
    /// Returns incompatible artifacts, transport or persistence errors.
    #[allow(clippy::too_many_lines)] // Replay/minimize share one original-preserving transaction.
    pub fn operation(
        &self,
        config: &ExecutionSettings,
        original: &str,
        mode: Reproduction,
    ) -> Result<ExecutionReport, ExecutionError> {
        let mode = mode.wire_mode();
        let mut store = ArtifactStore::open_matching(&config.output, &config.identity)
            .map_err(ExecutionError::Storage)?;
        let source = store
            .load_finding(original)
            .map_err(ExecutionError::Storage)?;
        let mut invocation = Invocation {
            test_id: source.test_id.clone(),
            raw_input: None,
        };
        let token = source.observation.replay_token.clone();
        let operation = if token.is_none() {
            if mode == "minimize" {
                return Err(io::Error::other(
                    "semantic minimization unavailable for a raw crash/timeout",
                )
                .into());
            }
            invocation.raw_input = Some(source.input.clone());
            "raw_replay"
        } else {
            mode
        };

        let mut report = ExecutionReport {
            version: 1,
            mode: mode.into(),
            ..ExecutionReport::default()
        };
        let start = Instant::now();
        let result = self.session(
            config,
            &invocation,
            operation,
            token.as_ref(),
            start,
            &mut store,
            &mut report,
            &mut None,
        )?;
        report.operation_observation.clone_from(&result.observation);
        let identity = FailureIdentity::saved(&source.observation);
        let mut verification = "pending";
        let mut paths = vec![];
        if let Some(observation) = &result.observation {
            verification = identity.check(Some(observation)).status();
            paths.push(
                store
                    .derived(
                        original,
                        &json!({"mode":mode,"observation":observation,"verification":verification}),
                    )
                    .map_err(ExecutionError::Storage)?,
            );
        }

        for mut evidence in result.derived {
            paths.push(
                store
                    .derived(original, &evidence)
                    .map_err(ExecutionError::Storage)?,
            );
            if mode == "minimize"
                && evidence.get("token").is_some()
                && !self.cancelled(config, start)
            {
                let candidate: Vec<u8> =
                    serde_json::from_value(evidence["token"].clone()).map_err(io::Error::from)?;
                let checked = self.session(
                    config,
                    &invocation,
                    "replay",
                    Some(&candidate),
                    start,
                    &mut store,
                    &mut report,
                    &mut None,
                )?;
                verification = FailureIdentity::semantic(source.observation.fingerprint.as_deref())
                    .check(checked.observation.as_ref())
                    .candidate_status();
                evidence["verification"] = json!(verification);
                report
                    .operation_observation
                    .clone_from(&checked.observation);
                evidence["observation"] =
                    serde_json::to_value(checked.observation).map_err(io::Error::from)?;
            }
            paths.push(
                store
                    .derived(original, &evidence)
                    .map_err(ExecutionError::Storage)?,
            );
        }
        report.outcomes.push(Outcome {
            test_id: invocation.test_id,
            case_id: 0,
            kind: source.observation.kind.clone(),
            original: Some(original.into()),
            derived: paths,
            verification: verification.into(),
            detail: None,
        });
        report.interrupted = self.interrupted.load(Ordering::Relaxed);
        report.elapsed_ms = start.elapsed().as_millis();
        Ok(report)
    }
    fn cancelled(&self, config: &ExecutionSettings, start: Instant) -> bool {
        self.interrupted.load(Ordering::Relaxed)
            || config
                .limits
                .campaign_ms
                .is_some_and(|m| start.elapsed() >= Duration::from_millis(m))
    }
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)] // one transport state machine, reviewed as a unit
    fn session(
        &self,
        config: &ExecutionSettings,
        invocation: &Invocation,
        mode: &str,
        token: Option<&Vec<u8>>,
        campaign_start: Instant,
        store: &mut ArtifactStore,
        report: &mut ExecutionReport,
        search: &mut Option<Search>,
    ) -> Result<RunResult, ExecutionError> {
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
                    detail: observation.rendered.clone(),
                });
                return Ok(RunResult {
                    observation: Some(observation),
                    original: None,
                    retained_input: None,
                    derived: vec![],
                });
            }
        };
        report.capabilities =
            serde_json::to_value(worker.resource.capabilities()).map_err(io::Error::from)?;
        let mut raw_dispatched = false;
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
            if let Some(error) = worker.send_error.take() {
                observed = Some(infrastructure(&error));
                break;
            }
            let event = if closed_at.is_some() {
                Err(mpsc::RecvTimeoutError::Disconnected)
            } else {
                worker.events.recv_timeout(Duration::from_millis(2))
            };
            match event {
                Ok(Ok(None)) => {
                    closed_at = Some(Instant::now());
                }
                Ok(Ok(Some(message))) => {
                    let kind = message["kind"].as_str().unwrap_or("");
                    match kind {
                        "ready" if !ready => {
                            ready = true;
                            let environment = message["environment"].clone();
                            if report.environment.is_null() {
                                report.environment = environment;
                            } else if report.environment != environment {
                                observed = Some(infrastructure("worker environment changed"));
                                break;
                            }
                            let budget = config.limits.campaign_ms.map_or(
                                Duration::MAX.as_secs_f64(),
                                |m| {
                                    (Duration::from_millis(m)
                                        .saturating_sub(campaign_start.elapsed()))
                                    .as_secs_f64()
                                },
                            );
                            worker.enqueue(json!({"kind": "start", "pytest_args": config.pytest_args, "test_id": invocation.test_id,
                                "mode": if mode == "raw_replay" { "run" } else { mode }, "token": token, "budget_seconds": budget}));
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
                                || (mode == "raw_replay" && raw_dispatched)
                            {
                                worker.enqueue(json!({"kind": "end"}));
                            } else {
                                let input = search.as_mut().map_or_else(
                                    || invocation.raw_input.clone().unwrap_or_default(),
                                    Search::next_case,
                                );
                                if mode == "run" {
                                    report.attempts += 1;
                                }
                                let id = if mode == "run" { report.attempts } else { 0 };
                                active = Some((id, input.clone(), Instant::now()));
                                raw_dispatched = true;
                                worker.enqueue(
                                    json!({"kind": "case", "case_id": id, "input": input}),
                                );
                            }
                        }
                        "replay_case" if ready && mode == "replay" && active.is_none() => {
                            if message["test_id"].as_str() != Some(&invocation.test_id) {
                                observed = Some(infrastructure("replay test ID mismatch"));
                                break;
                            }
                            active = Some((0, token.cloned().unwrap_or_default(), Instant::now()));
                            worker.enqueue(json!({"kind": "ack"}));
                        }
                        "skipped" if ready => {
                            if message["observation"]["nodeid"].as_str()
                                != Some(&invocation.test_id)
                                || active.as_ref().is_some_and(|(id, _, _)| {
                                    message["case_id"].as_u64() != Some(*id)
                                })
                            {
                                observed = Some(infrastructure("skip correlation mismatch"));
                                break;
                            }
                            active = None;
                            observed = Some(Observation {
                                kind: "skip".into(),
                                verification: "unstable".into(),
                                ..Observation::default()
                            });
                            if mode == "run" {
                                report.outcomes.push(Outcome {
                                    test_id: invocation.test_id.clone(),
                                    case_id: 0,
                                    kind: "skip".into(),
                                    original: None,
                                    derived: vec![],
                                    verification: "pending".into(),
                                    detail: None,
                                });
                            }
                            worker.enqueue(json!({"kind": "ack"}));
                        }
                        "result" if ready && mode != "minimize" => {
                            let Some((id, input, case_start)) = active.as_ref() else {
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
                                report.adapter_ns =
                                    report.adapter_ns.saturating_add(value.elapsed_ns);
                                report.dispatch_roundtrip_ns =
                                    report.dispatch_roundtrip_ns.saturating_add(
                                        u64::try_from(case_start.elapsed().as_nanos())
                                            .unwrap_or(u64::MAX),
                                    );
                                if let Some(search) = search.as_mut() {
                                    search.observe(input, &value);
                                }
                                if value.kind == "reject" {
                                    report.rejected += 1;
                                } else {
                                    report.useful += 1;
                                }
                                let new_finding = ["failure", "oom"].contains(&value.kind.as_str());
                                if new_finding {
                                    report
                                        .first_discovery_ms
                                        .get_or_insert(campaign_start.elapsed().as_millis());
                                    original = Some(
                                        store
                                            .record_original(
                                                &invocation.test_id,
                                                input,
                                                &serde_json::to_value(&value)
                                                    .map_err(io::Error::from)?,
                                            )
                                            .map_err(ExecutionError::Storage)?,
                                    );
                                    observed = Some(value.clone());
                                }
                                if report.outcomes.len() < 256 || new_finding {
                                    report.outcomes.push(Outcome {
                                        test_id: invocation.test_id.clone(),
                                        case_id: *id,
                                        kind: value.kind.clone(),
                                        original: original.clone(),
                                        derived: vec![],
                                        verification: "pending".into(),
                                        detail: value.rendered.clone(),
                                    });
                                }
                            } else {
                                observed = Some(value);
                            }
                            active = None;
                            worker.enqueue(json!({"kind": "ack"}));
                        }
                        "candidate" if ready && mode == "minimize" => {
                            let candidate: Vec<u8> =
                                serde_json::from_value(message["token"].clone())
                                    .map_err(io::Error::from)?;
                            // Retain the entire opaque token before authorizing its execution.
                            active = Some((0, candidate, Instant::now()));
                            worker.enqueue(json!({"kind": "ack"}));
                        }
                        "shrunk" if ready && mode == "minimize" => {
                            // Caller owns original lineage; return a separate record for this attempt.
                            derived.push(json!({"mode": "minimize", "token": message["token"], "complete": message["complete"], "verification": "pending"}));
                            active = None;
                        }
                        "done" if ready => {
                            if active.is_some() {
                                observed = Some(infrastructure("done with an active case"));
                            }
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
                    let status = poll_exit(&mut worker.child)?;
                    if status.is_none()
                        && closed_at.get_or_insert_with(Instant::now).elapsed()
                            < Duration::from_millis(20)
                    {
                        thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    if observed.is_some() {
                        observed = Some(infrastructure(
                            "worker exited before orderly session completion",
                        ));
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
        if mode == "run" {
            report.exploration_ms = report
                .exploration_ms
                .saturating_add(slice_start.map_or(0, |started| started.elapsed().as_millis()));
        }
        let retained_input = active.as_ref().map(|(_, input, _)| input.clone());
        worker.terminate();
        let log = worker.take_log();
        if report.logs.is_empty() {
            report.logs.push(log);
        } else {
            report.logs[0].merge(&log, config.limits.retained_output_bytes);
        }
        if let Some(value) = &observed {
            let failure = !(["pass", "reject", "skip", "failure"].contains(&value.kind.as_str())
                || value.kind == "oom" && original.is_some());
            if mode == "minimize"
                && let Some((_, candidate, _)) = &active
            {
                derived
                    .push(json!({"mode": "minimize", "active_token": candidate, "outcome": value}));
            }
            if failure
                && (mode == "run"
                    || ["infrastructure", "campaign_cancelled", "interrupted"]
                        .contains(&value.kind.as_str()))
            {
                if ["native_crash", "case_timeout", "oom"].contains(&value.kind.as_str()) {
                    report
                        .first_discovery_ms
                        .get_or_insert(campaign_start.elapsed().as_millis());
                }
                let id = active.as_ref().map_or(0, |(id, _, _)| *id);
                if mode == "run"
                    && let Some((_, input, _)) = active
                {
                    original = Some(
                        store
                            .record_original(
                                &invocation.test_id,
                                &input,
                                &serde_json::to_value(value).map_err(io::Error::from)?,
                            )
                            .map_err(ExecutionError::Storage)?,
                    );
                }
                report.outcomes.push(Outcome {
                    test_id: invocation.test_id.clone(),
                    case_id: id,
                    kind: value.kind.clone(),
                    original: original.clone(),
                    derived: vec![],
                    verification: "pending".into(),
                    detail: value.rendered.clone(),
                });
            }
        }
        if let Some(error) = &worker.cleanup_error {
            report.outcomes.push(Outcome {
                test_id: invocation.test_id.clone(),
                case_id: 0,
                kind: "infrastructure".into(),
                original: None,
                derived: vec![],
                verification: format!("cleanup failed: {error}"),
                detail: None,
            });
        }
        Ok(RunResult {
            observation: observed,
            original,
            retained_input,
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
fn poll_exit(child: &mut Child) -> io::Result<Option<std::process::ExitStatus>> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        // SAFETY: zero initializes a plain C output buffer. waitid populates a
        // SIGCHLD record; WNOWAIT keeps the root PID reserved until tree cleanup.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        if unsafe {
            libc::waitid(
                libc::P_PID,
                child.id(),
                &raw mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        } != 0
        {
            let error = io::Error::last_os_error();
            return if error.kind() == io::ErrorKind::Interrupted {
                Ok(None)
            } else {
                Err(error)
            };
        }
        // SAFETY: waitid returned successfully into the initialized SIGCHLD record.
        if unsafe { info.si_pid() } == 0 {
            return Ok(None);
        }
        // SAFETY: nonzero child PID identifies an initialized child-status payload.
        let value = unsafe { info.si_status() };
        let raw = match info.si_code {
            libc::CLD_EXITED => (value & 0xff) << 8,
            libc::CLD_KILLED => value & 0x7f,
            libc::CLD_DUMPED => (value & 0x7f) | 0x80,
            _ => return Err(io::Error::other("unexpected waitid notification")),
        };
        Ok(Some(std::process::ExitStatus::from_raw(raw)))
    }
    #[cfg(not(unix))]
    {
        child.try_wait()
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

/// Collection owns its wire schema and the identity used to admit artifact reuse.
#[derive(Debug, Deserialize)]
pub(crate) struct Discovery {
    #[serde(default)]
    nodeids: Vec<String>,
    #[serde(default)]
    environment: Value,
    #[serde(default)]
    sources: Value,
    #[serde(default)]
    implementation_sources: Value,
    #[serde(default)]
    error: Option<String>,
}
impl Discovery {
    pub(crate) fn targets(&self) -> &[String] {
        &self.nodeids
    }
    pub(crate) fn environment(&self) -> &Value {
        &self.environment
    }
    /// Whether pytest completed collection, including a successful empty selection.
    pub(crate) fn selection_complete(&self) -> bool {
        self.error.is_none()
    }
    pub(crate) fn admit(&self) -> io::Result<()> {
        if let Some(error) = &self.error {
            return Err(io::Error::other(format!(
                "pytest collection failed; no targets executed: {}",
                json!(error)
            )));
        }
        if self.nodeids.is_empty() {
            return Err(io::Error::other(
                "selection contains no supported Hypothesis tests",
            ));
        }
        Ok(())
    }
    pub(crate) fn identity(&self, selection: &[String]) -> Value {
        json!({"environment": self.environment, "sources": self.sources,
            "implementation_sources": self.implementation_sources, "selection": selection})
    }
}

/// Collect through a resource-attached child with bounded retained output.
/// # Errors
/// Returns collection errors, deadlines and unavailable hard containment.
#[allow(clippy::too_many_lines)] // One supervised startup handshake.
pub(crate) fn collect(
    python: &std::path::Path,
    args: &[String],
    limits: &Limits,
    interrupted: &AtomicBool,
) -> io::Result<Discovery> {
    use std::io::Write;
    let mut command = Command::new(python);
    command.args(["-I","-S","-c", "import sys; sys.stdin.buffer.read(1); import site; site.main(); sys.path.insert(0,sys.argv.pop(1)); import collect; collect.main()"])
        .arg(concat!(env!("CARGO_MANIFEST_DIR"),"/python/kiryl_adapter")).args(args)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn()?;
    let mut guard = match resource::attach(&mut child, limits, limits.allow_best_effort) {
        Ok(guard) => guard,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("missing collection stdout"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("missing collection stderr"))?;
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut bytes = vec![];
        let result = stdout.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes);
        let _ = sender.send(result.map(|_| bytes));
    });
    let log = Arc::new(Mutex::new(LogTail::default()));
    let tail = Arc::clone(&log);
    let retained = limits.retained_output_bytes;
    let (log_complete, log_ready) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut bytes = [0; 8192];
        while let Ok(count) = stderr.read(&mut bytes) {
            if count == 0 {
                break;
            }
            if let Ok(mut log) = tail.lock() {
                log.append(&bytes[..count], retained);
            }
        }
        let _ = log_complete.send(());
    });
    child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("missing collection stdin"))?
        .write_all(b"g")?;
    let start = Instant::now();
    let result = loop {
        if interrupted.load(Ordering::Relaxed)
            || limits
                .campaign_ms
                .is_some_and(|m| start.elapsed() >= Duration::from_millis(m))
        {
            break Err(io::Error::new(
                if interrupted.load(Ordering::Relaxed) {
                    io::ErrorKind::Interrupted
                } else {
                    io::ErrorKind::TimedOut
                },
                "collection interrupted or campaign budget exhausted",
            ));
        }
        match receiver.recv_timeout(Duration::from_millis(10)) {
            Ok(Ok(bytes)) if bytes.len() <= 8 * 1024 * 1024 => {
                break serde_json::from_slice(&bytes).map_err(io::Error::from);
            }
            Ok(Ok(_)) => break Err(io::Error::other("collection output exceeds limit")),
            Ok(Err(error)) => break Err(error),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break Err(io::Error::other("collection output disconnected"));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => (),
        }
    };
    guard.terminate_tree()?;
    let _ = child.kill();
    let _ = child.wait();
    let _ = log_ready.recv_timeout(Duration::from_millis(100));
    result.map_err(|error| {
        let bytes = log
            .lock()
            .map(|log| log.tail.iter().copied().collect::<Vec<_>>())
            .unwrap_or_default();
        io::Error::new(
            error.kind(),
            format!(
                "collection: {error}; stderr tail: {}",
                String::from_utf8_lossy(&bytes)
            ),
        )
    })
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
    proptest! {
        #[test]
        fn merged_tails_match_the_unbounded_reference(chunks in proptest::collection::vec(proptest::collection::vec(any::<u8>(), 0..256), 0..32), limit in 0usize..1024) {
            let mut aggregate = LogTail::default();
            let mut raw = Vec::new();
            for chunk in chunks {
                raw.extend_from_slice(&chunk);
                let mut source = LogTail::default();
                source.append(&chunk, limit);
                aggregate.merge(&source, limit);
            }
            let start = raw.len().saturating_sub(limit);
            prop_assert_eq!(aggregate.tail.into_iter().collect::<Vec<_>>(), &raw[start..]);
            prop_assert_eq!(aggregate.observed, raw.len() as u64);
        }
    }
    #[test]
    fn merging_truncated_tails_counts_full_stream_and_saturates() {
        let mut left = LogTail::default();
        left.append(b"abcdef", 2);
        let mut right = LogTail::default();
        right.append(b"ghij", 2);
        left.merge(&right, 3);
        assert_eq!(left.tail.iter().copied().collect::<Vec<_>>(), b"ij");
        assert_eq!(left.observed, 10);
        assert!(left.truncated);
        right.observed = u64::MAX;
        left.merge(&right, 0);
        assert_eq!(left.observed, u64::MAX);
        assert_eq!(left.tail.len(), 0);
        assert!(left.truncated);
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
    #[cfg(unix)]
    #[test]
    fn non_reaping_status_keeps_cleanup_anchor() {
        use std::os::unix::process::CommandExt;
        let mut child = Command::new("/bin/sh")
            .args(["-c", "exit 7"])
            .process_group(0)
            .spawn()
            .unwrap();
        let start = Instant::now();
        let status = loop {
            if let Some(status) = poll_exit(&mut child).unwrap() {
                break status;
            }
            assert!(start.elapsed() < Duration::from_secs(2));
            thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(status.code(), Some(7));
        let mut raw = 0;
        let pid = i32::try_from(child.id()).unwrap();
        // SAFETY: test owns this unreaped child; waitpid is deliberately the
        // first reaper and proves the supervisor preserved its cleanup anchor.
        assert_eq!(
            unsafe { libc::waitpid(pid, &raw mut raw, libc::WNOHANG) },
            pid
        );
    }
    #[test]
    fn reserved_baseline_statistics_stays_null_and_reproduction_modes_are_closed() {
        let report = serde_json::to_value(ExecutionReport::default()).unwrap();
        assert!(report["baseline_statistics"].is_null());
        assert_eq!(Reproduction::Replay.wire_mode(), "replay");
        assert_eq!(Reproduction::Minimize.wire_mode(), "minimize");
    }

    #[test]
    fn discovery_admits_only_supported_targets_and_keeps_identity_shape() {
        let discovery: Discovery = serde_json::from_value(json!({
            "nodeids": ["test_example.py::test_case"], "environment": {"python": [3, 14]},
            "sources": {"test_example.py": "hash"}, "implementation_sources": {"src/main.rs": "hash"}
        })).unwrap();
        discovery.admit().unwrap();
        assert_eq!(discovery.targets(), ["test_example.py::test_case"]);
        assert_eq!(discovery.environment(), &json!({"python": [3, 14]}));
        assert_eq!(
            discovery.identity(&["tests".into()]),
            json!({
                "environment": {"python": [3, 14]}, "sources": {"test_example.py": "hash"},
                "implementation_sources": {"src/main.rs": "hash"}, "selection": ["tests"]
            })
        );
        assert!(discovery.selection_complete());
        let empty: Discovery = serde_json::from_value(json!({"nodeids": []})).unwrap();
        assert!(empty.selection_complete());
        assert_eq!(
            empty.admit().unwrap_err().to_string(),
            "selection contains no supported Hypothesis tests"
        );
        let failed: Discovery =
            serde_json::from_value(json!({"nodeids": ["test"], "error": "collection failed"}))
                .unwrap();
        assert!(!failed.selection_complete());
        assert_eq!(
            failed.admit().unwrap_err().to_string(),
            "pytest collection failed; no targets executed: \"collection failed\""
        );
    }
    #[test]
    fn precedence() {
        assert_eq!(expiry(false, true, true), Some("campaign_cancelled"));
        assert_eq!(expiry(true, true, true), Some("interrupted"));
        assert_eq!(expiry(false, false, true), Some("case_timeout"));
    }
}
