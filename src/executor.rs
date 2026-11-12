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
    fn report_mode(self) -> &'static str {
        match self {
            Self::Replay => "replay",
            Self::Minimize => "minimize",
        }
    }
}

/// One admitted session operation. Semantic replay/minimization always carry
/// their opaque token, including a valid empty token. Raw replay reuses run's
/// worker protocol, but the supervisor dispatches only the retained input once.
#[derive(Clone, Copy, Debug)]
enum SessionOperation<'a> {
    Explore,
    Replay(&'a [u8]),
    Minimize(&'a [u8]),
    RawReplay,
}
impl SessionOperation<'_> {
    /// Encode the adapter start contract here so callers need no wire-mode or
    /// token-nullability knowledge. This does not authorize target execution.
    fn start(self, selection: &[String], test_id: &str, budget_seconds: f64) -> Value {
        let (mode, token) = match self {
            Self::Explore | Self::RawReplay => ("run", None),
            Self::Replay(token) => ("replay", Some(token)),
            Self::Minimize(token) => ("minimize", Some(token)),
        };
        json!({"kind":"start","pytest_args":selection,"test_id":test_id,
            "mode":mode,"token":token,"budget_seconds":budget_seconds})
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

impl ExecutionReport {
    // Cleanup diagnostics must not replace evidence already retained for a case.
    fn record_cleanup_failure(&mut self, target: &str, error: Option<&str>) {
        if let Some(error) = error {
            self.outcomes.push(Outcome {
                test_id: target.into(),
                case_id: 0,
                kind: "infrastructure".into(),
                original: None,
                derived: vec![],
                verification: format!("cleanup failed: {error}"),
                detail: None,
            });
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

// A piped Command requests these handles; preserve a precise infrastructure
// diagnostic if a caller ever supplies a child without a required pipe.
fn required_pipe<T>(pipe: Option<T>, diagnostic: &'static str) -> io::Result<T> {
    pipe.ok_or_else(|| io::Error::other(diagnostic))
}

// Once supervision drops its receiver, stop reading the worker's stream rather
// than draining output that no remaining owner can classify or retain.
fn read_events(output: &mut impl Read, send: &mpsc::SyncSender<io::Result<Option<Value>>>) {
    loop {
        match wire::read(output) {
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
        let mut input = required_pipe(child.stdin.take(), "missing stdin")?;
        let mut output = required_pipe(child.stdout.take(), "missing stdout")?;
        let mut errors = required_pipe(child.stderr.take(), "missing stderr")?;
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
        thread::spawn(move || read_events(&mut output, &send));
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
        let mut search = Search::new(config.seed);
        search.restore(store.load_corpus().map_err(ExecutionError::Storage)?);
        let result = self.session(
            &config,
            invocation,
            SessionOperation::Explore,
            start,
            &mut store,
            &mut report,
            Some(&mut search),
        )?;
        if let Some(observed) = result.observation {
            let index = report
                .outcomes
                .iter()
                .rposition(|o| o.original == result.original && o.kind == observed.kind)
                .unwrap_or(report.outcomes.len().saturating_sub(1));
            if observed.kind == "failure"
                && let (Some(token), Some(original)) = (&observed.replay_token, &result.original)
                && !self.cancelled(&config, start)
            {
                let replay_start = Instant::now();
                let verified = self.session(
                    &config,
                    invocation,
                    SessionOperation::Replay(token),
                    start,
                    &mut store,
                    &mut report,
                    None,
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
                        SessionOperation::Minimize(token),
                        start,
                        &mut store,
                        &mut report,
                        None,
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
                                SessionOperation::Replay(&candidate),
                                start,
                                &mut store,
                                &mut report,
                                None,
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
                    SessionOperation::RawReplay,
                    start,
                    &mut store,
                    &mut report,
                    None,
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
        store
            .save_corpus(search.corpus())
            .map_err(ExecutionError::Storage)?;
        report.branches = search.feature_count();
        report.corpus_entries = search.corpus_len();
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
        let mut store = ArtifactStore::open_matching(&config.output, &config.identity)
            .map_err(ExecutionError::Storage)?;
        let source = store
            .load_finding(original)
            .map_err(ExecutionError::Storage)?;
        let mut invocation = Invocation {
            test_id: source.test_id.clone(),
            raw_input: None,
        };
        let operation = match (mode, source.observation.replay_token.as_deref()) {
            (Reproduction::Replay, Some(token)) => SessionOperation::Replay(token),
            (Reproduction::Minimize, Some(token)) => SessionOperation::Minimize(token),
            (Reproduction::Minimize, None) => {
                return Err(io::Error::other(
                    "semantic minimization unavailable for a raw crash/timeout",
                )
                .into());
            }
            (Reproduction::Replay, None) => {
                invocation.raw_input = Some(source.input.clone());
                SessionOperation::RawReplay
            }
        };

        let mut report = ExecutionReport {
            version: 1,
            mode: mode.report_mode().into(),
            ..ExecutionReport::default()
        };
        let start = Instant::now();
        let result = self.session(
            config,
            &invocation,
            operation,
            start,
            &mut store,
            &mut report,
            None,
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
                        &json!({"mode":mode.report_mode(),"observation":observation,"verification":verification}),
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
            if matches!(mode, Reproduction::Minimize)
                && evidence.get("token").is_some()
                && !self.cancelled(config, start)
            {
                let candidate: Vec<u8> =
                    serde_json::from_value(evidence["token"].clone()).map_err(io::Error::from)?;
                let checked = self.session(
                    config,
                    &invocation,
                    SessionOperation::Replay(&candidate),
                    start,
                    &mut store,
                    &mut report,
                    None,
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
    /// Supervise one typed operation, preserving active input before acknowledging
    /// execution and consuming worker cleanup authority before returning evidence.
    fn session(
        &self,
        config: &ExecutionSettings,
        invocation: &Invocation,
        mode: SessionOperation<'_>,
        campaign_start: Instant,
        store: &mut ArtifactStore,
        report: &mut ExecutionReport,
        mut search: Option<&mut Search>,
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
                if let Some(observation) =
                    resource_observation(worker.resource.oom_observed(), true)
                {
                    observed = Some(observation);
                    break;
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
                    match (kind, mode) {
                        ("ready", _) if !ready => {
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
                            worker.enqueue(mode.start(
                                &config.pytest_args,
                                &invocation.test_id,
                                budget,
                            ));
                        }
                        ("next", SessionOperation::Explore | SessionOperation::RawReplay)
                            if ready && active.is_none() =>
                        {
                            if message["test_id"].as_str() != Some(&invocation.test_id) {
                                observed = Some(infrastructure("test ID mismatch"));
                                break;
                            }
                            if slice_start.get_or_insert_with(Instant::now).elapsed()
                                >= Duration::from_millis(config.limits.slice_ms)
                                || observed.is_some()
                                || (matches!(mode, SessionOperation::RawReplay) && raw_dispatched)
                            {
                                worker.enqueue(json!({"kind": "end"}));
                            } else {
                                let input = search.as_deref_mut().map_or_else(
                                    || invocation.raw_input.clone().unwrap_or_default(),
                                    Search::next_case,
                                );
                                if matches!(mode, SessionOperation::Explore) {
                                    report.attempts += 1;
                                }
                                let id = if matches!(mode, SessionOperation::Explore) {
                                    report.attempts
                                } else {
                                    0
                                };
                                active = Some((id, input.clone(), Instant::now()));
                                raw_dispatched = true;
                                worker.enqueue(
                                    json!({"kind": "case", "case_id": id, "input": input}),
                                );
                            }
                        }
                        ("replay_case", SessionOperation::Replay(token))
                            if ready && active.is_none() =>
                        {
                            if message["test_id"].as_str() != Some(&invocation.test_id) {
                                observed = Some(infrastructure("replay test ID mismatch"));
                                break;
                            }
                            active = Some((0, token.to_vec(), Instant::now()));
                            worker.enqueue(json!({"kind": "ack"}));
                        }
                        ("skipped", _) if ready => {
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
                            if matches!(mode, SessionOperation::Explore) {
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
                        (
                            "result",
                            SessionOperation::Explore
                            | SessionOperation::Replay(_)
                            | SessionOperation::RawReplay,
                        ) if ready => {
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
                            let Some(value) =
                                admit_observation(value, worker.resource.oom_observed()?)
                            else {
                                observed = Some(infrastructure("unknown observation"));
                                break;
                            };
                            if matches!(mode, SessionOperation::Explore) {
                                report.adapter_ns =
                                    report.adapter_ns.saturating_add(value.elapsed_ns);
                                report.dispatch_roundtrip_ns =
                                    report.dispatch_roundtrip_ns.saturating_add(
                                        u64::try_from(case_start.elapsed().as_nanos())
                                            .unwrap_or(u64::MAX),
                                    );
                                if let Some(search) = search.as_deref_mut() {
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
                        ("candidate", SessionOperation::Minimize(_)) if ready => {
                            let candidate: Vec<u8> =
                                serde_json::from_value(message["token"].clone())
                                    .map_err(io::Error::from)?;
                            // Retain the entire opaque token before authorizing its execution.
                            active = Some((0, candidate, Instant::now()));
                            worker.enqueue(json!({"kind": "ack"}));
                        }
                        ("shrunk", SessionOperation::Minimize(_)) if ready => {
                            // Caller owns original lineage; return a separate record for this attempt.
                            derived.push(json!({"mode": "minimize", "token": message["token"], "complete": message["complete"], "verification": "pending"}));
                            active = None;
                        }
                        ("done", _) if ready => {
                            if active.is_some() {
                                observed = Some(infrastructure("done with an active case"));
                            }
                            break;
                        }
                        ("error", _) => {
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
                    if observed.is_none() {
                        observed = resource_observation(
                            Ok(worker.resource.oom_observed()?),
                            active.is_some(),
                        );
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
        if matches!(mode, SessionOperation::Explore) {
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
            if matches!(mode, SessionOperation::Minimize(_))
                && let Some((_, candidate, _)) = &active
            {
                derived
                    .push(json!({"mode": "minimize", "active_token": candidate, "outcome": value}));
            }
            if failure
                && (matches!(mode, SessionOperation::Explore)
                    || ["infrastructure", "campaign_cancelled", "interrupted"]
                        .contains(&value.kind.as_str()))
            {
                if ["native_crash", "case_timeout", "oom"].contains(&value.kind.as_str()) {
                    report
                        .first_discovery_ms
                        .get_or_insert(campaign_start.elapsed().as_millis());
                }
                let id = active.as_ref().map_or(0, |(id, _, _)| *id);
                if matches!(mode, SessionOperation::Explore)
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
        report.record_cleanup_failure(&invocation.test_id, worker.cleanup_error.as_deref());
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
            return polling_error(io::Error::last_os_error());
        }
        // SAFETY: waitid returned successfully into the initialized SIGCHLD record.
        if unsafe { info.si_pid() } == 0 {
            return Ok(None);
        }
        // SAFETY: nonzero child PID identifies an initialized child-status payload.
        let value = unsafe { info.si_status() };
        decode_unix_exit(info.si_code, value).map(Some)
    }
    #[cfg(not(unix))]
    {
        child.try_wait()
    }
}

/// Interrupted nonblocking notification polls remain pending; other OS failures
/// retain their original diagnostic rather than looking like a live process.
#[cfg(unix)]
fn polling_error(error: io::Error) -> io::Result<Option<std::process::ExitStatus>> {
    if error.kind() == io::ErrorKind::Interrupted {
        Ok(None)
    } else {
        Err(error)
    }
}

/// Translate the waitid SIGCHLD payload without reaping the process-group anchor.
/// Only WEXITED notifications are admitted; raw status masks match waitpid ABI.
#[cfg(unix)]
fn decode_unix_exit(code: i32, value: i32) -> io::Result<std::process::ExitStatus> {
    use std::os::unix::process::ExitStatusExt;
    let raw = match code {
        libc::CLD_EXITED => (value & 0xff) << 8,
        libc::CLD_KILLED => value & 0x7f,
        libc::CLD_DUMPED => (value & 0x7f) | 0x80,
        _ => return Err(io::Error::other("unexpected waitid notification")),
    };
    Ok(std::process::ExitStatus::from_raw(raw))
}

// Positive backend evidence takes precedence over a worker's result label while
// retaining its diagnostic and reproduction payload. Unknown labels need evidence.
fn admit_observation(mut value: Observation, oom: bool) -> Option<Observation> {
    if oom {
        value.kind = "oom".into();
    }
    ["pass", "reject", "failure", "oom"]
        .contains(&value.kind.as_str())
        .then_some(value)
}

// No positive evidence leaves classification open. OOM during an active case is
// a case finding; without an active case it is infrastructure. Periodic polling
// supplies errors here as infrastructure observations. EOF callers propagate
// counter-read errors with `?` before calling this policy, preserving transport errors.
fn resource_observation(evidence: io::Result<bool>, active: bool) -> Option<Observation> {
    match evidence {
        Ok(false) => None,
        Ok(true) => Some(Observation {
            kind: if active { "oom" } else { "infrastructure" }.into(),
            verification: "pending".into(),
            ..Observation::default()
        }),
        Err(error) => Some(infrastructure(&format!("resource evidence: {error}"))),
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

// Collection output is bounded and decoded only once its reader finishes;
// timeout leaves supervision active, while loss of the reader is infrastructure.
fn collection_event(
    event: Result<io::Result<Vec<u8>>, mpsc::RecvTimeoutError>,
) -> std::ops::ControlFlow<io::Result<Discovery>> {
    let result = match event {
        Ok(Ok(bytes)) if bytes.len() <= 8 * 1024 * 1024 => {
            serde_json::from_slice(&bytes).map_err(io::Error::from)
        }
        Ok(Ok(_)) => Err(io::Error::other("collection output exceeds limit")),
        Ok(Err(error)) => Err(error),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err(io::Error::other("collection output disconnected"))
        }
        Err(mpsc::RecvTimeoutError::Timeout) => return std::ops::ControlFlow::Continue(()),
    };
    std::ops::ControlFlow::Break(result)
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
    let stdout = required_pipe(child.stdout.take(), "missing collection stdout")?;
    let mut stderr = required_pipe(child.stderr.take(), "missing collection stderr")?;
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
    required_pipe(child.stdin.take(), "missing collection stdin")?.write_all(b"g")?;
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
        if let std::ops::ControlFlow::Break(result) =
            collection_event(receiver.recv_timeout(Duration::from_millis(10)))
        {
            break result;
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
    #[cfg(unix)]
    struct SessionFixture {
        root: PathBuf,
        config: ExecutionSettings,
    }
    #[cfg(unix)]
    impl SessionFixture {
        fn new(frames: &[Value]) -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "kiryl-worker-unit-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&root).unwrap();
            let frame_path = root.join("frames");
            let mut bytes = Vec::new();
            frames
                .iter()
                .try_for_each(|frame| wire::write(&mut bytes, frame.clone()))
                .unwrap();
            std::fs::write(&frame_path, bytes).unwrap();
            let python = root.join("worker");
            let script = if frames.is_empty() {
                format!("#!/bin/sh\ncat '{}'\nsleep 60\n", frame_path.display())
            } else {
                format!(
                    "#!/bin/sh\n{}",
                    Self::command_script(&root, "default", frames)
                )
            };
            std::fs::write(&python, script).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&python, std::fs::Permissions::from_mode(0o700)).unwrap();
            }
            let config = ExecutionSettings {
                python,
                pytest_args: vec![],
                seed: 1,
                output: root.join("artifacts"),
                identity: json!({}),
                limits: Limits {
                    campaign_ms: Some(30_000),
                    case_ms: 10_000,
                    allow_best_effort: true,
                    ..Limits::default()
                },
            };
            Self { root, config }
        }
        fn command_script(root: &std::path::Path, label: &str, frames: &[Value]) -> String {
            use std::fmt::Write as _;
            let mut script = String::from(
                "read_command() {\nset -- $(dd bs=1 count=4 2>/dev/null | od -An -tu1)\nlength=$(( $1 * 16777216 + $2 * 65536 + $3 * 256 + $4 ))\ndd bs=1 count=\"$length\" of=/dev/null 2>/dev/null\n}\n",
            );
            frames.iter().enumerate().for_each(|(frame_index, frame)| {
                let path = root.join(format!("frame-{label}-{frame_index}"));
                let mut bytes = Vec::new();
                wire::write(&mut bytes, frame.clone()).unwrap();
                std::fs::write(&path, bytes).unwrap();
                writeln!(script, "cat '{}'", path.display()).unwrap();
                if matches!(
                    frame["kind"].as_str(),
                    Some("ready" | "next" | "replay_case" | "candidate" | "result" | "skipped")
                ) {
                    script.push_str("read_command\n");
                }
            });
            script.push_str("sleep 60\n");
            script
        }
        fn sequenced(sessions: &[Vec<Value>]) -> Self {
            let fixture = Self::new(&[]);
            sessions.iter().enumerate().for_each(|(index, frames)| {
                let script = Self::command_script(&fixture.root, &(index + 1).to_string(), frames);
                std::fs::write(fixture.root.join(format!("session-{}", index + 1)), script)
                    .unwrap();
            });
            std::fs::write(fixture.root.join("launch-count"), b"0").unwrap();
            std::fs::write(&fixture.config.python, format!(
                "#!/bin/sh\nread count < '{root}/launch-count'\ncount=$((count + 1))\nprintf '%s\\n' \"$count\" > '{root}/launch-count'\nexec /bin/sh '{root}/session-'\"$count\"\n",
                root=fixture.root.display(),
            )).unwrap();
            fixture
        }
        fn saved_original(&self, replay_token: Option<&[u8]>) -> String {
            let mut store =
                ArtifactStore::open_matching(&self.config.output, &self.config.identity).unwrap();
            store.record_original("test", &[4,2], &json!({"kind":"failure","fingerprint":"same","rendered":"original","replay_token":replay_token})).unwrap()
        }
        fn run(&self, mode: SessionOperation<'_>, report: &mut ExecutionReport) -> RunResult {
            let mut store =
                ArtifactStore::open_matching(&self.config.output, &self.config.identity).unwrap();
            Executor::new(Arc::new(AtomicBool::new(false)))
                .session(
                    &self.config,
                    &Invocation {
                        test_id: "test".into(),
                        raw_input: Some(vec![4, 2]),
                    },
                    mode,
                    Instant::now(),
                    &mut store,
                    report,
                    None,
                )
                .unwrap()
        }
    }
    #[cfg(unix)]
    impl Drop for SessionFixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.root).unwrap();
        }
    }

    #[cfg(target_os = "linux")]
    struct NativeCgroupFault {
        root: PathBuf,
        control: &'static str,
        parent: PathBuf,
    }
    #[cfg(target_os = "linux")]
    impl NativeCgroupFault {
        fn install(
            fixture: &mut SessionFixture,
            launch: usize,
            control: &'static str,
            keep_alive: bool,
        ) -> Self {
            assert!(
                std::env::var_os("KIRYL_CGROUP_PARENT").is_some(),
                "native fault tests require delegated cgroups"
            );
            fixture.config.limits.allow_best_effort = false;
            let script_path = fixture.root.join(format!("session-{launch}"));
            let script = std::fs::read_to_string(&script_path).unwrap();
            let start = script.strip_suffix("sleep 60\n").unwrap();
            let tail = if keep_alive {
                let path = fixture.root.join("fault-ready");
                let mut bytes = Vec::new();
                wire::write(&mut bytes, json!({"kind":"fixture_fault_ready"})).unwrap();
                std::fs::write(&path, bytes).unwrap();
                format!("cat '{}'\nexec /bin/sleep 60\n", path.display())
            } else {
                "exit 0\n".into()
            };
            let fault = format!(
                "set -eu\nmembership=$(sed -n 's/^0:://p' /proc/self/cgroup)\nname=${{membership##*/}}\ncase \"$name\" in kiryl-probe-{pid}-*) ;; *) exit 81 ;; esac\nowned=\"$KIRYL_CGROUP_PARENT/$name\"\nstat -c '%a' \"$owned/{control}\" > '{root}/fault-mode'\nprintf '%s\\n' \"$owned\" > '{root}/fault-cgroup'\nchmod 000 \"$owned/{control}\"\n{tail}",
                pid = std::process::id(),
                root = fixture.root.display(),
            );
            std::fs::write(script_path, format!("{start}{fault}")).unwrap();
            Self {
                root: fixture.root.clone(),
                control,
                parent: std::fs::canonicalize(std::env::var_os("KIRYL_CGROUP_PARENT").unwrap())
                    .unwrap(),
            }
        }
        fn cleanup(&self) -> io::Result<()> {
            self.cleanup_until(Instant::now() + Duration::from_secs(5))
        }
        fn cleanup_until(&self, deadline: Instant) -> io::Result<()> {
            use std::os::unix::fs::PermissionsExt;
            let marker = self.root.join("fault-cgroup");
            let group = match std::fs::read_to_string(marker) {
                Ok(path) => PathBuf::from(path.trim()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(error),
            };
            if group.parent() != Some(self.parent.as_path())
                || !group
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(&format!("kiryl-probe-{}-", std::process::id()))
            {
                return Err(io::Error::other(
                    "refusing cleanup outside the fixture's owned cgroup",
                ));
            }
            if !group.try_exists()? {
                return Ok(());
            }
            let original = std::fs::read_to_string(self.root.join("fault-mode"))?;
            let mode = u32::from_str_radix(original.trim(), 8).map_err(io::Error::other)?;
            std::fs::set_permissions(
                group.join(self.control),
                std::fs::Permissions::from_mode(mode),
            )?;
            // This concrete directory was identified by the fixture worker's own
            // /proc membership after the parent's acknowledged attachment.
            std::fs::write(group.join("cgroup.kill"), b"1")?;
            loop {
                let result = std::fs::remove_dir(&group);
                if result.is_ok() || !group.try_exists()? {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return result;
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
    #[cfg(target_os = "linux")]
    impl Drop for NativeCgroupFault {
        fn drop(&mut self) {
            let _ = self.cleanup();
        }
    }
    #[cfg(target_os = "linux")]
    fn native_cleanup_fixture() -> (SessionFixture, NativeCgroupFault, PathBuf) {
        let fixture = SessionFixture::new(&[]);
        let parent = fixture.root.join("owned-parent");
        std::fs::create_dir(&parent).unwrap();
        let parent = std::fs::canonicalize(parent).unwrap();
        let group = parent.join(format!("kiryl-probe-{}-fixture", std::process::id()));
        std::fs::create_dir(&group).unwrap();
        std::fs::write(fixture.root.join("fault-cgroup"), group.to_str().unwrap()).unwrap();
        std::fs::write(fixture.root.join("fault-mode"), "600").unwrap();
        std::fs::write(group.join("memory.events"), "owned counter").unwrap();
        std::fs::write(group.join("cgroup.kill"), "0").unwrap();
        let fault = NativeCgroupFault {
            root: fixture.root.clone(),
            control: "memory.events",
            parent,
        };
        (fixture, fault, group)
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn native_fault_cleanup_without_marker_has_no_effect() {
        let (_fixture, fault, group) = native_cleanup_fixture();
        std::fs::remove_file(fault.root.join("fault-cgroup")).unwrap();
        fault.cleanup().unwrap();
        assert_eq!(
            std::fs::read_to_string(group.join("cgroup.kill")).unwrap(),
            "0"
        );
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn native_fault_cleanup_marker_read_error_is_visible() {
        let (_fixture, fault, group) = native_cleanup_fixture();
        let marker = fault.root.join("fault-cgroup");
        std::fs::remove_file(&marker).unwrap();
        std::fs::create_dir(&marker).unwrap();
        assert_eq!(
            fault.cleanup().unwrap_err().kind(),
            io::ErrorKind::IsADirectory
        );
        assert_eq!(
            std::fs::read_to_string(group.join("cgroup.kill")).unwrap(),
            "0"
        );
        std::fs::remove_dir(marker).unwrap();
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn native_fault_cleanup_rejects_a_different_parent() {
        let (fixture, fault, _group) = native_cleanup_fixture();
        let outside = fixture.root.join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("cgroup.kill"), "preserved").unwrap();
        std::fs::write(fault.root.join("fault-cgroup"), outside.to_str().unwrap()).unwrap();
        assert_eq!(
            fault.cleanup().unwrap_err().to_string(),
            "refusing cleanup outside the fixture's owned cgroup"
        );
        assert_eq!(
            std::fs::read_to_string(outside.join("cgroup.kill")).unwrap(),
            "preserved"
        );
        std::fs::remove_file(fault.root.join("fault-cgroup")).unwrap();
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn native_fault_cleanup_rejects_an_unowned_name() {
        let (_fixture, fault, _group) = native_cleanup_fixture();
        let outside = fault.parent.join("foreign");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("cgroup.kill"), "preserved").unwrap();
        std::fs::write(fault.root.join("fault-cgroup"), outside.to_str().unwrap()).unwrap();
        assert_eq!(
            fault.cleanup().unwrap_err().to_string(),
            "refusing cleanup outside the fixture's owned cgroup"
        );
        assert_eq!(
            std::fs::read_to_string(outside.join("cgroup.kill")).unwrap(),
            "preserved"
        );
        std::fs::remove_file(fault.root.join("fault-cgroup")).unwrap();
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn native_fault_cleanup_accepts_an_already_removed_group() {
        let (_fixture, fault, group) = native_cleanup_fixture();
        std::fs::remove_dir_all(&group).unwrap();
        fault.cleanup().unwrap();
        assert!(!group.exists());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn native_fault_cleanup_rejects_an_invalid_permission_record() {
        let (_fixture, fault, group) = native_cleanup_fixture();
        std::fs::write(fault.root.join("fault-mode"), "not-octal").unwrap();
        assert!(
            fault
                .cleanup()
                .unwrap_err()
                .to_string()
                .contains("invalid digit")
        );
        assert_eq!(
            std::fs::read_to_string(group.join("cgroup.kill")).unwrap(),
            "0"
        );
        std::fs::remove_file(fault.root.join("fault-cgroup")).unwrap();
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn native_fault_cleanup_reports_missing_permission_record() {
        let (_fixture, fault, group) = native_cleanup_fixture();
        std::fs::remove_file(fault.root.join("fault-mode")).unwrap();
        assert_eq!(fault.cleanup().unwrap_err().kind(), io::ErrorKind::NotFound);
        assert_eq!(
            std::fs::read_to_string(group.join("cgroup.kill")).unwrap(),
            "0"
        );
        std::fs::remove_file(fault.root.join("fault-cgroup")).unwrap();
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn native_fault_cleanup_reports_permission_restore_failure() {
        let (_fixture, fault, group) = native_cleanup_fixture();
        std::fs::remove_file(group.join("memory.events")).unwrap();
        assert_eq!(fault.cleanup().unwrap_err().kind(), io::ErrorKind::NotFound);
        assert_eq!(
            std::fs::read_to_string(group.join("cgroup.kill")).unwrap(),
            "0"
        );
        std::fs::remove_file(fault.root.join("fault-cgroup")).unwrap();
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn native_fault_cleanup_reports_kill_control_write_failure() {
        let (_fixture, fault, group) = native_cleanup_fixture();
        std::fs::remove_file(group.join("cgroup.kill")).unwrap();
        std::fs::create_dir(group.join("cgroup.kill")).unwrap();
        assert_eq!(
            fault.cleanup().unwrap_err().kind(),
            io::ErrorKind::IsADirectory
        );
        std::fs::remove_file(fault.root.join("fault-cgroup")).unwrap();
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn native_fault_cleanup_stops_at_deadline_after_removal_failure() {
        let (_fixture, fault, group) = native_cleanup_fixture();
        assert_eq!(
            fault
                .cleanup_until(Instant::now() + Duration::from_millis(100))
                .unwrap_err()
                .kind(),
            io::ErrorKind::DirectoryNotEmpty
        );
        assert_eq!(
            std::fs::read_to_string(group.join("cgroup.kill")).unwrap(),
            "1"
        );
        std::fs::remove_file(fault.root.join("fault-cgroup")).unwrap();
    }
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires delegated cgroup"]
    fn native_active_resource_counter_failure_is_infrastructure_evidence() {
        let mut fixture = SessionFixture::sequenced(&[vec![
            json!({"kind":"ready","environment":{}}),
            json!({"kind":"next","test_id":"test"}),
        ]]);
        let fault = NativeCgroupFault::install(&mut fixture, 1, "memory.events", true);
        let script = fixture.root.join("session-1");
        let body = std::fs::read_to_string(&script).unwrap().replace(
            &format!("cat '{}'\n", fixture.root.join("fault-ready").display()),
            "",
        );
        std::fs::write(script, body).unwrap();
        let report = Executor::new(Arc::new(AtomicBool::new(false)))
            .explore("test", fixture.config.clone())
            .unwrap();
        fault.cleanup().unwrap();
        let outcome = report.outcomes.last().unwrap();
        assert_eq!(outcome.kind, "infrastructure");
        assert!(
            outcome
                .detail
                .as_deref()
                .unwrap()
                .starts_with("resource evidence: ")
        );
        assert!(outcome.original.is_some());
    }
    #[cfg(target_os = "linux")]
    fn native_failure_frames(replay: bool) -> Vec<Value> {
        let request = if replay {
            json!({"kind":"replay_case","test_id":"test"})
        } else {
            json!({"kind":"next","test_id":"test"})
        };
        vec![
            json!({"kind":"ready","environment":{}}),
            request,
            json!({"kind":"result","case_id":u64::from(!replay),"observation":{"nodeid":"test","kind":"failure","fingerprint":"same","replay_token":[9]}}),
            json!({"kind":"done"}),
        ]
    }
    #[cfg(target_os = "linux")]
    fn native_shrink_frames() -> Vec<Value> {
        vec![
            json!({"kind":"ready","environment":{}}),
            json!({"kind":"candidate","token":[2]}),
            json!({"kind":"shrunk","token":[2],"complete":true}),
            json!({"kind":"done"}),
        ]
    }
    #[cfg(target_os = "linux")]
    #[rstest::rstest]
    #[case::failure_replay(2)]
    #[case::failure_minimization(3)]
    #[case::minimized_candidate_replay(4)]
    #[ignore = "requires delegated cgroup"]
    fn native_resource_counter_error_propagates_from_exploration_reproduction(
        #[case] launch: usize,
    ) {
        let mut sessions = vec![
            native_failure_frames(false),
            native_failure_frames(true),
            native_shrink_frames(),
            vec![],
        ];
        sessions.truncate(launch);
        sessions[launch - 1] = vec![json!({"kind":"ready","environment":{}})];
        let mut fixture = SessionFixture::sequenced(&sessions);
        let fault = NativeCgroupFault::install(&mut fixture, launch, "memory.events", false);
        let result =
            Executor::new(Arc::new(AtomicBool::new(false))).explore("test", fixture.config.clone());
        fault.cleanup().unwrap();
        assert_eq!(
            result.unwrap_err().to_string(),
            ExecutionError::Transport(io::Error::from_raw_os_error(libc::EACCES)).to_string()
        );
    }
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires delegated cgroup"]
    fn native_resource_counter_error_propagates_from_operation_candidate_replay() {
        let mut fixture = SessionFixture::sequenced(&[
            native_shrink_frames(),
            vec![json!({"kind":"ready","environment":{}})],
        ]);
        let fault = NativeCgroupFault::install(&mut fixture, 2, "memory.events", false);
        let original = fixture.saved_original(Some(&[9]));
        let result = Executor::new(Arc::new(AtomicBool::new(false))).operation(
            &fixture.config,
            &original,
            Reproduction::Minimize,
        );
        fault.cleanup().unwrap();
        assert_eq!(
            result.unwrap_err().to_string(),
            ExecutionError::Transport(io::Error::from_raw_os_error(libc::EACCES)).to_string()
        );
    }
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires delegated cgroup"]
    fn native_termination_failure_remains_sticky_after_authority_consumed() {
        let mut fixture =
            SessionFixture::sequenced(&[vec![json!({"kind":"ready","environment":{}})]]);
        let fault = NativeCgroupFault::install(&mut fixture, 1, "cgroup.kill", true);
        let mut worker = Worker::launch(&fixture.config).unwrap();
        assert_eq!(
            worker
                .events
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap()
                .unwrap()["kind"],
            "ready"
        );
        worker.enqueue(json!({"kind":"start"}));
        assert_eq!(
            worker
                .events
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap()
                .unwrap()["kind"],
            "fixture_fault_ready"
        );
        worker.terminate();
        let first = worker.cleanup_error.clone().unwrap();
        worker.terminate();
        fault.cleanup().unwrap();
        assert_eq!(worker.cleanup_error.as_deref(), Some(first.as_str()));
        assert_eq!(
            first,
            io::Error::from_raw_os_error(libc::EACCES).to_string()
        );
    }
    #[cfg(unix)]
    #[rstest::rstest]
    #[case(SessionOperation::Explore, vec![json!({"kind":"next","test_id":"other"})], "test ID mismatch")]
    #[case(SessionOperation::Replay(&[9]), vec![json!({"kind":"replay_case","test_id":"other"})], "replay test ID mismatch")]
    #[case(SessionOperation::Explore, vec![json!({"kind":"skipped","observation":{"nodeid":"other"}})], "skip correlation mismatch")]
    #[case(SessionOperation::Explore, vec![json!({"kind":"result"})], "result without active case")]
    #[case(SessionOperation::Explore, vec![json!({"kind":"next","test_id":"test"}),json!({"kind":"result","case_id":99,"observation":{"nodeid":"test"}})], "case ID mismatch")]
    #[case(SessionOperation::Explore, vec![json!({"kind":"next","test_id":"test"}),json!({"kind":"result","case_id":1,"observation":{"nodeid":"test","kind":4}})], "malformed observation")]
    #[case(SessionOperation::Explore, vec![json!({"kind":"next","test_id":"test"}),json!({"kind":"result","case_id":1,"observation":{"nodeid":"test","kind":"unknown"}})], "unknown observation")]
    #[case(SessionOperation::Explore, vec![json!({"kind":"next","test_id":"test"}),json!({"kind":"done"})], "done with an active case")]
    #[case(SessionOperation::Explore, vec![json!({"kind":"error","message":"injected failure"})], "injected failure")]
    #[case(SessionOperation::Explore, vec![json!({"kind":"error"})], "worker error")]
    #[case(SessionOperation::Explore, vec![json!({"kind":"unexpected"})], "unexpected control frame")]
    fn invalid_worker_protocol_is_infrastructure(
        #[case] mode: SessionOperation<'_>,
        #[case] frames: Vec<Value>,
        #[case] expected: &str,
    ) {
        let frames = [vec![json!({"kind":"ready","environment":{}})], frames].concat();
        let fixture = SessionFixture::new(&frames);
        let result = fixture.run(mode, &mut ExecutionReport::default());
        let observation = result.observation.unwrap();
        assert_eq!(observation.kind, "infrastructure");
        assert_eq!(observation.rendered.as_deref(), Some(expected));
    }
    #[cfg(unix)]
    #[test]
    fn changed_worker_environment_is_infrastructure() {
        let fixture = SessionFixture::new(&[json!({"kind":"ready","environment":{"python":2}})]);
        let mut report = ExecutionReport {
            environment: json!({"python":1}),
            ..ExecutionReport::default()
        };
        let result = fixture.run(SessionOperation::Explore, &mut report);
        assert_eq!(
            result.observation.unwrap().rendered.as_deref(),
            Some("worker environment changed")
        );
    }
    #[cfg(unix)]
    #[rstest::rstest]
    #[case(SessionOperation::Explore)]
    #[case(SessionOperation::Replay(&[9]))]
    fn correlated_skip_finishes_orderly_session(#[case] mode: SessionOperation<'_>) {
        let fixture = SessionFixture::new(&[
            json!({"kind":"ready","environment":{}}),
            json!({"kind":"skipped","observation":{"nodeid":"test"}}),
            json!({"kind":"done"}),
        ]);
        let result = fixture.run(mode, &mut ExecutionReport::default());
        assert_eq!(result.observation.unwrap().kind, "skip");
    }
    #[cfg(unix)]
    #[test]
    fn missing_interpreter_is_setup_infrastructure() {
        let mut fixture = SessionFixture::new(&[]);
        fixture.config.python = fixture.root.join("missing");
        let result = fixture.run(SessionOperation::Explore, &mut ExecutionReport::default());
        let observation = result.observation.unwrap();
        assert_eq!(observation.kind, "infrastructure");
        assert!(
            observation
                .rendered
                .unwrap()
                .starts_with("worker/resource setup:")
        );
    }
    #[cfg(unix)]
    #[rstest::rstest]
    #[case(Limits {tree_memory_bytes:0, ..Limits::default()})]
    #[case(Limits {case_ms:0, ..Limits::default()})]
    #[case(Limits {slice_ms:0, ..Limits::default()})]
    #[case(Limits {retained_output_bytes:16 * 1024 * 1024 + 1, ..Limits::default()})]
    fn invalid_executor_settings_fail_before_launch(#[case] limits: Limits) {
        let mut fixture = SessionFixture::new(&[]);
        fixture.config.limits = limits;
        let result =
            Executor::new(Arc::new(AtomicBool::new(false))).explore("test", fixture.config.clone());
        assert_eq!(
            result.unwrap_err().to_string(),
            "configuration: invalid executor settings"
        );
    }

    #[cfg(unix)]
    #[test]
    fn enqueue_after_command_channel_closes_records_error() {
        let fixture = SessionFixture::new(&[]);
        let mut worker = Worker::launch(&fixture.config).unwrap();
        worker.input.take();
        worker.enqueue(json!({"kind":"start"}));
        assert_eq!(worker.send_error.as_deref(), Some("closed command channel"));
    }
    #[cfg(unix)]
    #[rstest::rstest]
    #[case(false, Some(0), io::ErrorKind::TimedOut)]
    #[case(true, None, io::ErrorKind::Interrupted)]
    fn collection_expiry_reports_reason(
        #[case] interrupted: bool,
        #[case] campaign_ms: Option<u64>,
        #[case] expected: io::ErrorKind,
    ) {
        let mut fixture = SessionFixture::new(&[]);
        fixture.config.limits.campaign_ms = campaign_ms;
        let result = collect(
            &fixture.config.python,
            &[],
            &fixture.config.limits,
            &AtomicBool::new(interrupted),
        );
        assert_eq!(result.unwrap_err().kind(), expected);
    }
    #[cfg(unix)]
    #[rstest::rstest]
    #[case(b"not json".to_vec(), "expected ident")]
    #[case(vec![b'x'; 8 * 1024 * 1024 + 1], "collection output exceeds limit")]
    fn invalid_collection_output_preserves_stderr(#[case] bytes: Vec<u8>, #[case] expected: &str) {
        let fixture = SessionFixture::new(&[]);
        std::fs::write(fixture.root.join("frames"), bytes).unwrap();
        std::fs::write(
            &fixture.config.python,
            format!(
                "#!/bin/sh\ndd bs=1 count=1 >/dev/null 2>&1\nprintf 'collection diagnostic' >&2\ncat '{}'\n",
                fixture.root.join("frames").display()
            ),
        )
        .unwrap();
        let error = collect(
            &fixture.config.python,
            &[],
            &fixture.config.limits,
            &AtomicBool::new(false),
        )
        .unwrap_err();
        let detail = error.to_string();
        assert!(detail.contains(expected), "{detail}");
        assert!(detail.contains("collection diagnostic"), "{detail}");
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn mandatory_hard_memory_unavailable_reports_infrastructure() {
        let mut fixture = SessionFixture::new(&[]);
        fixture.config.limits.allow_best_effort = false;
        let report = Executor::new(Arc::new(AtomicBool::new(false)))
            .explore("test", fixture.config.clone())
            .unwrap();
        assert_eq!(
            report.outcomes[0].verification,
            "hard memory backend unavailable"
        );
        assert_eq!(report.outcomes[0].kind, "infrastructure");
    }
    #[cfg(unix)]
    #[test]
    fn interrupted_minimization_retains_active_candidate() {
        let fixture = SessionFixture::new(&[
            json!({"kind":"ready","environment":{}}),
            json!({"kind":"candidate","token":[1,2,3]}),
            json!({"kind":"error","message":"candidate interrupted"}),
        ]);
        let result = fixture.run(
            SessionOperation::Minimize(&[9]),
            &mut ExecutionReport::default(),
        );
        assert_eq!(result.observation.unwrap().kind, "infrastructure");
        assert_eq!(result.derived[0]["active_token"], json!([1, 2, 3]));
        assert_eq!(
            result.derived[0]["outcome"]["rendered"],
            "candidate interrupted"
        );
    }
    #[cfg(unix)]
    #[test]
    fn replay_handshake_accepts_correlated_result() {
        let fixture = SessionFixture::new(&[
            json!({"kind":"ready","environment":{}}),
            json!({"kind":"replay_case","test_id":"test"}),
            json!({"kind":"result","case_id":0,"observation":{"nodeid":"test","kind":"pass"}}),
            json!({"kind":"done"}),
        ]);
        let result = fixture.run(
            SessionOperation::Replay(&[9]),
            &mut ExecutionReport::default(),
        );
        let observation = result.observation.unwrap();
        assert_eq!(observation.kind, "pass", "{observation:?}");
        assert!(result.retained_input.is_none());
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn unavailable_worker_resource_attachment_reports_setup_error() {
        let mut fixture = SessionFixture::new(&[]);
        fixture.config.limits.allow_best_effort = false;
        let result = fixture.run(SessionOperation::Explore, &mut ExecutionReport::default());
        assert!(
            result
                .observation
                .unwrap()
                .rendered
                .unwrap()
                .contains("no native hard memory backend")
        );
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn unavailable_collection_resource_attachment_fails_closed() {
        let mut fixture = SessionFixture::new(&[]);
        fixture.config.limits.allow_best_effort = false;
        let error = collect(
            &fixture.config.python,
            &[],
            &fixture.config.limits,
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    }
    #[cfg(unix)]
    #[test]
    fn incompatible_worker_payload_is_transport_infrastructure() {
        let fixture = SessionFixture::new(&[]);
        std::fs::write(fixture.root.join("frames"), [0, 0, 0, 2, b'[', b']']).unwrap();
        let result = fixture.run(SessionOperation::Explore, &mut ExecutionReport::default());
        assert_eq!(
            result.observation.unwrap().rendered.as_deref(),
            Some("incompatible frame")
        );
    }
    #[cfg(unix)]
    #[test]
    fn worker_closing_command_input_reports_broken_pipe() {
        let fixture = SessionFixture::new(&[json!({"kind":"ready","environment":{}})]);
        std::fs::write(
            &fixture.config.python,
            format!(
                "#!/bin/sh\nexec 0<&-\ncat '{}'\nsleep 60\n",
                fixture.root.join("frames").display()
            ),
        )
        .unwrap();
        let result = fixture.run(SessionOperation::Explore, &mut ExecutionReport::default());
        assert!(
            result
                .observation
                .unwrap()
                .rendered
                .unwrap()
                .to_lowercase()
                .contains("broken pipe")
        );
    }
    #[cfg(unix)]
    fn replay_frames(kind: &str, fingerprint: &str) -> Vec<Value> {
        let mut frames = vec![json!({"kind":"ready","environment":{}})];
        match kind {
            "none" => (),
            "skip" => frames.push(json!({"kind":"skipped","observation":{"nodeid":"test"}})),
            "infrastructure" => frames.push(json!({"kind":"error","message":"verification unavailable"})),
            _ => frames.extend([
                json!({"kind":"replay_case","test_id":"test"}),
                json!({"kind":"result","case_id":0,"observation":{"nodeid":"test","kind":kind,"fingerprint":fingerprint}}),
            ]),
        }
        frames.push(json!({"kind":"done"}));
        frames
    }
    #[cfg(unix)]
    fn shrink_frames(token: &Value) -> Vec<Value> {
        vec![
            json!({"kind":"ready","environment":{}}),
            json!({"kind":"candidate","token":[8]}),
            json!({"kind":"shrunk","token":token,"complete":true}),
            json!({"kind":"done"}),
        ]
    }
    #[cfg(unix)]
    fn initial_failure_frames() -> Vec<Value> {
        vec![
            json!({"kind":"ready","environment":{}}),
            json!({"kind":"next","test_id":"test"}),
            json!({"kind":"result","case_id":1,"observation":{"nodeid":"test","kind":"failure","fingerprint":"same","rendered":"original","replay_token":[9]}}),
            json!({"kind":"done"}),
        ]
    }
    #[cfg(unix)]
    fn persisted_evidence(path: &str) -> Value {
        let value: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        value["evidence"].clone()
    }
    #[cfg(unix)]
    #[rstest::rstest]
    #[case::confirmed("failure", "same", "confirmed")]
    #[case::different_assertion("failure", "different", "unstable")]
    #[case::passing("pass", "same", "unstable")]
    #[case::skipped("skip", "same", "unstable")]
    #[case::infrastructure("infrastructure", "same", "pending")]
    #[case::incomplete("none", "same", "pending")]
    fn exploration_preserves_pending_and_checked_shrink_evidence(
        #[case] kind: &str,
        #[case] fingerprint: &str,
        #[case] expected: &str,
    ) {
        let fixture = SessionFixture::sequenced(&[
            initial_failure_frames(),
            replay_frames("failure", "same"),
            shrink_frames(&json!([8])),
            replay_frames(kind, fingerprint),
        ]);
        let report = Executor::new(Arc::new(AtomicBool::new(false)))
            .explore("test", fixture.config.clone())
            .unwrap();
        let outcome = &report.outcomes[0];
        assert_eq!(outcome.verification, "confirmed");
        assert_eq!(outcome.derived.len(), 3);
        assert_eq!(
            persisted_evidence(&outcome.derived[1])["verification"],
            "pending"
        );
        assert_eq!(
            persisted_evidence(&outcome.derived[2])["verification"],
            expected
        );
        assert_eq!(persisted_evidence(&outcome.derived[2])["token"], json!([8]));
        let original: Value =
            serde_json::from_slice(&std::fs::read(outcome.original.as_ref().unwrap()).unwrap())
                .unwrap();
        assert_eq!(original["observation"]["rendered"], "original");
        assert_eq!(original["observation"]["replay_token"], json!([9]));
    }
    #[cfg(unix)]
    #[rstest::rstest]
    #[case::matching("failure", "same", "confirmed")]
    #[case::mismatched("failure", "different", "unstable")]
    #[case::passing("pass", "same", "unstable")]
    #[case::skipped("skip", "same", "unstable")]
    #[case::infrastructure("infrastructure", "same", "pending")]
    #[case::incomplete("none", "same", "pending")]
    fn standalone_minimization_checks_candidate_in_fresh_session(
        #[case] kind: &str,
        #[case] fingerprint: &str,
        #[case] expected: &str,
    ) {
        let fixture = SessionFixture::sequenced(&[
            shrink_frames(&json!([8])),
            replay_frames(kind, fingerprint),
        ]);
        let original = fixture.saved_original(Some(&[9]));
        let before = std::fs::read(&original).unwrap();
        let report = Executor::new(Arc::new(AtomicBool::new(false)))
            .operation(&fixture.config, &original, Reproduction::Minimize)
            .unwrap();
        let outcome = report.outcomes.last().unwrap();
        assert_eq!(
            outcome.verification,
            expected,
            "{}",
            serde_json::to_string(&report).unwrap()
        );
        assert_eq!(
            outcome.derived.len(),
            2,
            "{}",
            serde_json::to_string(&report).unwrap()
        );
        assert_eq!(
            persisted_evidence(&outcome.derived[0])["verification"],
            "pending"
        );
        assert_eq!(
            persisted_evidence(&outcome.derived[1])["verification"],
            expected
        );
        assert_eq!(std::fs::read(&original).unwrap(), before);
    }
    #[cfg(unix)]
    #[test]
    fn tokenless_original_cannot_undergo_semantic_minimization() {
        let fixture = SessionFixture::new(&[]);
        let original = fixture.saved_original(None);
        let before = std::fs::read(&original).unwrap();
        let error = Executor::new(Arc::new(AtomicBool::new(false)))
            .operation(&fixture.config, &original, Reproduction::Minimize)
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("semantic minimization unavailable")
        );
        assert_eq!(std::fs::read(original).unwrap(), before);
    }
    #[cfg(unix)]
    #[test]
    fn malformed_shrunk_token_cannot_authorize_fresh_replay() {
        let fixture = SessionFixture::sequenced(&[shrink_frames(&json!("invalid"))]);
        let original = fixture.saved_original(Some(&[9]));
        let error = Executor::new(Arc::new(AtomicBool::new(false)))
            .operation(&fixture.config, &original, Reproduction::Minimize)
            .unwrap_err();
        assert!(error.to_string().contains("invalid type"));
        assert_eq!(
            std::fs::read_to_string(fixture.root.join("launch-count"))
                .unwrap()
                .trim(),
            "1"
        );
    }
    #[cfg(unix)]
    #[test]
    fn full_command_queue_records_backpressure_error() {
        let fixture = SessionFixture::new(&[]);
        let mut worker = Worker::launch(&fixture.config).unwrap();
        let (sender, _receiver) = mpsc::sync_channel(1);
        sender.try_send(json!({"kind":"occupied"})).unwrap();
        worker.input = Some(sender);
        worker.enqueue(json!({"kind":"next"}));
        assert!(worker.send_error.as_ref().unwrap().contains("full"));
    }
    #[cfg(unix)]
    #[test]
    fn disconnected_command_queue_records_writer_loss() {
        let fixture = SessionFixture::new(&[]);
        let mut worker = Worker::launch(&fixture.config).unwrap();
        let (sender, receiver) = mpsc::sync_channel(1);
        drop(receiver);
        worker.input = Some(sender);
        worker.enqueue(json!({"kind":"next"}));
        assert!(worker.send_error.as_ref().unwrap().contains("closed"));
    }
    #[cfg(unix)]
    #[test]
    fn poisoned_log_storage_returns_empty_tail() {
        let fixture = SessionFixture::new(&[]);
        let worker = Worker::launch(&fixture.config).unwrap();
        let log = Arc::clone(&worker.log);
        let panic = std::panic::catch_unwind(move || {
            let _guard = log.lock().unwrap();
            panic!("injected log owner panic");
        });
        assert!(panic.is_err());
        assert_eq!(worker.take_log().observed, 0);
    }
    #[rstest::rstest]
    #[case::no_oom(Ok(false), true, None)]
    #[case::observed_oom(Ok(true), true, Some(("oom", "pending", None)))]
    #[case::unavailable_evidence(Err(io::Error::new(io::ErrorKind::PermissionDenied, "resource denied")), true, Some(("infrastructure", "pending", Some("resource evidence: resource denied"))))]
    #[case::oom_before_case(Ok(true), false, Some(("infrastructure", "pending", None)))]
    fn resource_evidence_policy_preserves_classification(
        #[case] evidence: io::Result<bool>,
        #[case] active: bool,
        #[case] expected: Option<(&str, &str, Option<&str>)>,
    ) {
        let actual = resource_observation(evidence, active).map(|observation| {
            (
                observation.kind,
                observation.verification,
                observation.rendered,
            )
        });
        let expected = expected.map(|(kind, verification, rendered)| {
            (
                kind.to_owned(),
                verification.to_owned(),
                rendered.map(str::to_owned),
            )
        });
        assert_eq!(actual, expected);
    }
    #[rstest::rstest]
    #[case::clean_termination(None, 1, None)]
    #[case::failed_termination(Some("cleanup denied"), 2, Some("cleanup failed: cleanup denied"))]
    fn cleanup_failure_keeps_existing_candidate_and_records_infrastructure(
        #[case] error: Option<&str>,
        #[case] count: usize,
        #[case] verification: Option<&str>,
    ) {
        let mut report = ExecutionReport {
            outcomes: vec![Outcome {
                test_id: "test".into(),
                case_id: 7,
                kind: "failure".into(),
                original: Some("finding-0.bin".into()),
                derived: vec!["evidence-1.bin".into()],
                verification: "pending".into(),
                detail: Some("original assertion".into()),
            }],
            ..ExecutionReport::default()
        };
        let original = serde_json::to_value(&report.outcomes[0]).unwrap();
        report.record_cleanup_failure("test", error);
        assert_eq!(report.outcomes.len(), count);
        assert_eq!(serde_json::to_value(&report.outcomes[0]).unwrap(), original);
        let cleanup = report.outcomes.get(1);
        assert_eq!(
            cleanup.map(|outcome| outcome.verification.as_str()),
            verification
        );
        assert_eq!(
            cleanup.map(|outcome| outcome.kind.as_str()),
            verification.map(|_| "infrastructure")
        );
        assert_eq!(
            cleanup.map(|outcome| outcome.test_id.as_str()),
            verification.map(|_| "test")
        );
        assert_eq!(
            cleanup.map(|outcome| outcome.case_id),
            verification.map(|_| 0)
        );
        assert!(
            cleanup
                .and_then(|outcome| outcome.original.as_ref())
                .is_none()
        );
        assert!(
            cleanup
                .and_then(|outcome| outcome.detail.as_ref())
                .is_none()
        );
        assert_eq!(
            cleanup.map(|outcome| outcome.derived.len()),
            verification.map(|_| 0)
        );
    }
    #[test]
    fn required_spawn_pipe_preserves_the_supplied_handle() {
        let handle = String::from("owned handle");
        assert_eq!(
            required_pipe(Some(handle), "missing pipe").unwrap(),
            "owned handle"
        );
    }
    #[rstest::rstest]
    #[case::worker_stdin("missing stdin")]
    #[case::worker_stdout("missing stdout")]
    #[case::worker_stderr("missing stderr")]
    #[case::collection_stdin("missing collection stdin")]
    #[case::collection_stdout("missing collection stdout")]
    #[case::collection_stderr("missing collection stderr")]
    fn absent_spawn_pipe_preserves_exact_diagnostic(#[case] diagnostic: &'static str) {
        let error = required_pipe::<String>(None, diagnostic).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(error.to_string(), diagnostic);
    }
    #[test]
    fn complete_collection_event_preserves_discovery_identity() {
        let bytes = serde_json::to_vec(
            &json!({"nodeids":["test"],"environment":{"python":3},"sources":{"test":"digest"}}),
        )
        .unwrap();
        let discovery = collection_event(Ok(Ok(bytes)))
            .break_value()
            .unwrap()
            .unwrap();
        assert_eq!(discovery.targets(), ["test"]);
        assert_eq!(discovery.environment(), &json!({"python":3}));
        assert_eq!(
            discovery.identity(&["test.py".into()]),
            json!({"environment":{"python":3},"sources":{"test":"digest"},"implementation_sources":null,"selection":["test.py"]})
        );
    }
    #[rstest::rstest]
    #[case::malformed(Ok(Ok(b"not json".to_vec())), io::ErrorKind::InvalidData, "expected ident")]
    #[case::oversized(Ok(Ok(vec![b'x'; 8*1024*1024+1])), io::ErrorKind::Other, "collection output exceeds limit")]
    #[case::read_failure(
        Ok(Err(io::Error::new(io::ErrorKind::PermissionDenied, "read denied"))),
        io::ErrorKind::PermissionDenied,
        "read denied"
    )]
    #[case::disconnected(
        Err(mpsc::RecvTimeoutError::Disconnected),
        io::ErrorKind::Other,
        "collection output disconnected"
    )]
    fn terminal_collection_event_preserves_error_classification(
        #[case] event: Result<io::Result<Vec<u8>>, mpsc::RecvTimeoutError>,
        #[case] kind: io::ErrorKind,
        #[case] diagnostic: &str,
    ) {
        let error = collection_event(event).break_value().unwrap().unwrap_err();
        assert_eq!(error.kind(), kind);
        assert!(error.to_string().contains(diagnostic), "{error}");
    }
    #[test]
    fn collection_timeout_keeps_waiting_for_output() {
        assert!(collection_event(Err(mpsc::RecvTimeoutError::Timeout)).is_continue());
    }
    #[test]
    fn reader_stops_consuming_frames_when_event_owner_disappears() {
        let mut bytes = Vec::new();
        wire::write(&mut bytes, json!({"kind":"first"})).unwrap();
        let first_length = bytes.len();
        wire::write(&mut bytes, json!({"kind":"second"})).unwrap();
        let mut output = io::Cursor::new(bytes);
        let (sender, receiver) = mpsc::sync_channel(1);
        drop(receiver);
        read_events(&mut output, &sender);
        assert_eq!(output.position(), u64::try_from(first_length).unwrap());
    }
    #[cfg(unix)]
    #[rstest::rstest]
    #[case::matching(1, "skip", None)]
    #[case::mismatched(2, "infrastructure", Some(vec![4,2]))]
    fn active_skip_must_correlate_with_the_admitted_case(
        #[case] case_id: u64,
        #[case] kind: &str,
        #[case] retained: Option<Vec<u8>>,
    ) {
        let fixture = SessionFixture::new(&[
            json!({"kind":"ready","environment":{}}),
            json!({"kind":"next","test_id":"test"}),
            json!({"kind":"skipped","case_id":case_id,"observation":{"nodeid":"test"}}),
            json!({"kind":"done"}),
        ]);
        let result = fixture.run(SessionOperation::Explore, &mut ExecutionReport::default());
        let observation = result.observation.unwrap();
        assert_eq!(observation.kind, kind, "{observation:?}");
        assert_eq!(result.retained_input, retained);
        assert_eq!(result.original.is_some(), retained.is_some());
    }
    #[cfg(unix)]
    #[test]
    fn failure_acknowledgement_does_not_replace_orderly_session_completion() {
        let fixture = SessionFixture::new(&[
            json!({"kind":"ready","environment":{}}),
            json!({"kind":"next","test_id":"test"}),
            json!({"kind":"result","case_id":1,"observation":{"nodeid":"test","kind":"failure","fingerprint":"same"}}),
        ]);
        let script = std::fs::read_to_string(&fixture.config.python)
            .unwrap()
            .replace("sleep 60\n", "exit 0\n");
        std::fs::write(&fixture.config.python, script).unwrap();
        let result = fixture.run(SessionOperation::Explore, &mut ExecutionReport::default());
        let observation = result.observation.unwrap();
        assert_eq!(observation.kind, "infrastructure", "{observation:?}");
        assert_eq!(
            observation.rendered.as_deref(),
            Some("worker exited before orderly session completion")
        );
        let record: Value =
            serde_json::from_slice(&std::fs::read(result.original.unwrap()).unwrap()).unwrap();
        assert_eq!(record["input"], json!([4, 2]));
        assert_eq!(record["observation"]["kind"], "failure");
    }
    #[cfg(unix)]
    #[test]
    fn worker_stdout_eof_while_alive_retains_active_input_after_grace() {
        let fixture = SessionFixture::new(&[
            json!({"kind":"ready","environment":{}}),
            json!({"kind":"next","test_id":"test"}),
        ]);
        let script = std::fs::read_to_string(&fixture.config.python)
            .unwrap()
            .replace("sleep 60\n", "exec 1>&-\nsleep 60\n");
        std::fs::write(&fixture.config.python, script).unwrap();
        let result = fixture.run(SessionOperation::Explore, &mut ExecutionReport::default());
        let observation = result.observation.unwrap();
        assert_eq!(observation.kind, "infrastructure", "{observation:?}");
        assert_eq!(
            observation.rendered.as_deref(),
            Some("worker exited before done")
        );
        assert_eq!(result.retained_input, Some(vec![4, 2]));
        let record: Value =
            serde_json::from_slice(&std::fs::read(result.original.unwrap()).unwrap()).unwrap();
        assert_eq!(record["input"], json!([4, 2]));
    }
    #[cfg(unix)]
    fn unsuccessful_shrink_frames(candidate: &Value) -> Vec<Value> {
        vec![
            json!({"kind":"ready","environment":{}}),
            json!({"kind":"candidate","token":candidate}),
            json!({"kind":"error","message":"candidate verification unavailable"}),
        ]
    }
    #[cfg(unix)]
    #[test]
    fn exploration_storage_error_preserves_another_pending_original() {
        let fixture = SessionFixture::sequenced(&[initial_failure_frames()]);
        let store =
            ArtifactStore::open_matching(&fixture.config.output, &fixture.config.identity).unwrap();
        let pending = fixture.config.output.join("finding-0.bin.pending");
        std::fs::write(&pending, b"another writer's pending original").unwrap();
        drop(store);
        let error = Executor::new(Arc::new(AtomicBool::new(false)))
            .explore("test", fixture.config.clone())
            .unwrap_err();
        assert!(error.to_string().starts_with("storage:"), "{error}");
        assert_eq!(
            std::fs::read(pending).unwrap(),
            b"another writer's pending original"
        );
        assert!(!fixture.config.output.join("finding-0.bin").exists());
    }
    #[cfg(unix)]
    #[test]
    fn automatic_minimization_malformed_candidate_preserves_confirmed_original() {
        let fixture = SessionFixture::sequenced(&[
            initial_failure_frames(),
            replay_frames("failure", "same"),
            unsuccessful_shrink_frames(&json!("invalid")),
        ]);
        let error = Executor::new(Arc::new(AtomicBool::new(false)))
            .explore("test", fixture.config.clone())
            .unwrap_err();
        assert!(error.to_string().contains("invalid type"), "{error}");
        let original: Value = serde_json::from_slice(
            &std::fs::read(fixture.config.output.join("finding-0.bin")).unwrap(),
        )
        .unwrap();
        assert_eq!(original["observation"]["kind"], "failure");
        assert_eq!(original["observation"]["replay_token"], json!([9]));
        assert_eq!(
            std::fs::read_to_string(fixture.root.join("launch-count"))
                .unwrap()
                .trim(),
            "3"
        );
    }
    #[cfg(unix)]
    #[test]
    fn standalone_minimization_malformed_candidate_preserves_saved_record() {
        let fixture = SessionFixture::sequenced(&[unsuccessful_shrink_frames(&json!("invalid"))]);
        let original = fixture.saved_original(Some(&[9]));
        let before = std::fs::read(&original).unwrap();
        let error = Executor::new(Arc::new(AtomicBool::new(false)))
            .operation(&fixture.config, &original, Reproduction::Minimize)
            .unwrap_err();
        assert!(error.to_string().contains("invalid type"), "{error}");
        assert_eq!(std::fs::read(original).unwrap(), before);
    }
    #[cfg(unix)]
    #[rstest::rstest]
    #[case::different_assertion("failure", "different", "unstable", 1)]
    #[case::passing("pass", "same", "unstable", 1)]
    #[case::skipped("skip", "same", "unstable", 1)]
    #[case::rejected("reject", "same", "unstable", 1)]
    #[case::infrastructure("infrastructure", "same", "pending", 1)]
    #[case::incomplete("none", "same", "pending", 0)]
    fn unsuccessful_initial_verification_never_starts_minimization(
        #[case] kind: &str,
        #[case] fingerprint: &str,
        #[case] expected: &str,
        #[case] evidence_count: usize,
    ) {
        let fixture = SessionFixture::sequenced(&[
            initial_failure_frames(),
            replay_frames(kind, fingerprint),
        ]);
        let report = Executor::new(Arc::new(AtomicBool::new(false)))
            .explore("test", fixture.config.clone())
            .unwrap();
        let outcome = &report.outcomes[0];
        assert_eq!(outcome.verification, expected);
        assert_eq!(outcome.derived.len(), evidence_count);
        assert_eq!(
            std::fs::read_to_string(fixture.root.join("launch-count"))
                .unwrap()
                .trim(),
            "2"
        );
        let original: Value =
            serde_json::from_slice(&std::fs::read(outcome.original.as_ref().unwrap()).unwrap())
                .unwrap();
        assert_eq!(original["observation"]["fingerprint"], "same");
    }
    #[cfg(unix)]
    #[test]
    fn automatic_unfinished_shrink_keeps_active_token_without_fresh_replay() {
        let fixture = SessionFixture::sequenced(&[
            initial_failure_frames(),
            replay_frames("failure", "same"),
            unsuccessful_shrink_frames(&json!([8])),
        ]);
        let report = Executor::new(Arc::new(AtomicBool::new(false)))
            .explore("test", fixture.config.clone())
            .unwrap();
        let outcome = &report.outcomes[0];
        assert_eq!(outcome.verification, "confirmed");
        assert_eq!(outcome.derived.len(), 3);
        let retained = persisted_evidence(&outcome.derived[2]);
        assert_eq!(retained["active_token"], json!([8]));
        assert_eq!(retained["outcome"]["kind"], "infrastructure");
        assert!(retained.get("token").is_none());
        assert_eq!(
            std::fs::read_to_string(fixture.root.join("launch-count"))
                .unwrap()
                .trim(),
            "3"
        );
    }
    #[cfg(unix)]
    #[test]
    fn standalone_unfinished_shrink_keeps_active_token_without_fresh_replay() {
        let fixture = SessionFixture::sequenced(&[unsuccessful_shrink_frames(&json!([8]))]);
        let original = fixture.saved_original(Some(&[9]));
        let before = std::fs::read(&original).unwrap();
        let report = Executor::new(Arc::new(AtomicBool::new(false)))
            .operation(&fixture.config, &original, Reproduction::Minimize)
            .unwrap();
        let outcome = report.outcomes.last().unwrap();
        assert_eq!(outcome.verification, "pending");
        assert_eq!(outcome.derived.len(), 3);
        let retained = persisted_evidence(&outcome.derived[2]);
        assert_eq!(retained["active_token"], json!([8]));
        assert_eq!(retained["outcome"]["kind"], "infrastructure");
        assert!(retained.get("token").is_none());
        assert_eq!(std::fs::read(original).unwrap(), before);
        assert_eq!(
            std::fs::read_to_string(fixture.root.join("launch-count"))
                .unwrap()
                .trim(),
            "1"
        );
    }
    proptest! {
        #[test]
        fn tail_preserves_exact_suffix(chunks in proptest::collection::vec(proptest::collection::vec(any::<u8>(), 0..256), 0..32), limit in 0usize..1024) {
            let all: Vec<u8> = chunks.concat();
            let tail = chunks.iter().fold(LogTail::default(), |mut tail, chunk| {
                tail.append(chunk, limit);
                tail
            });
            let start = all.len().saturating_sub(limit);
            prop_assert_eq!(tail.tail.into_iter().collect::<Vec<_>>(), &all[start..]);
            prop_assert_eq!(tail.observed, all.len() as u64);
            prop_assert_eq!(tail.truncated, all.len() > limit);
        }
    }
    proptest! {
        #[test]
        fn merged_tails_match_the_unbounded_reference(chunks in proptest::collection::vec(proptest::collection::vec(any::<u8>(), 0..256), 0..32), limit in 0usize..1024) {
            let raw = chunks.concat();
            let aggregate = chunks.iter().fold(LogTail::default(), |mut aggregate, chunk| {
                let mut source = LogTail::default();
                source.append(chunk, limit);
                aggregate.merge(&source, limit);
                aggregate
            });
            let start = raw.len().saturating_sub(limit);
            prop_assert_eq!(aggregate.tail.into_iter().collect::<Vec<_>>(), &raw[start..]);
            prop_assert_eq!(aggregate.observed, raw.len() as u64);
            prop_assert_eq!(aggregate.truncated, raw.len() > limit);
        }
    }
    #[test]
    fn merging_truncated_tails_counts_full_stream() {
        let mut left = LogTail::default();
        left.append(b"abcdef", 2);
        let mut right = LogTail::default();
        right.append(b"ghij", 2);
        left.merge(&right, 3);
        assert_eq!(left.tail.iter().copied().collect::<Vec<_>>(), b"ij");
        assert_eq!(left.observed, 10);
        assert!(left.truncated);
    }
    #[test]
    fn merging_tails_saturates_observed_count() {
        let mut left = LogTail::default();
        left.append(b"abcdef", 2);
        let mut right = LogTail::default();
        right.append(b"ghij", 2);
        right.observed = u64::MAX;
        left.merge(&right, 0);
        assert_eq!(left.observed, u64::MAX);
        assert_eq!(left.tail.len(), 0);
        assert!(left.truncated);
    }
    #[test]
    fn budget_correspondence_parameterization_covers_corpus() {
        assert_eq!(
            include_str!("../formal/budget-cases.tsv").lines().count(),
            8
        );
    }
    #[rstest::rstest]
    fn lean_generated_budget_correspondence(#[values(0, 1, 2, 3, 4, 5, 6, 7)] row: usize) {
        let line = include_str!("../formal/budget-cases.tsv")
            .lines()
            .nth(row)
            .unwrap();
        let fields: Vec<_> = line.split('\t').collect();
        let actual = expiry(
            fields[0] == "true",
            fields[1] == "true",
            fields[2] == "true",
        )
        .unwrap_or("running");
        assert_eq!(actual, fields[3]);
    }
    #[rstest::rstest]
    #[case(3, true)]
    #[case(-1_073_741_819, true)]
    #[case(7, false)]
    #[case(0, false)]
    fn windows_abnormal_exit_table_is_candidate_evidence_only(
        #[case] code: i32,
        #[case] expected: bool,
    ) {
        assert_eq!(windows_abnormal_exit(code), expected);
    }
    #[cfg(unix)]
    fn await_exit(child: &mut Child) -> std::process::ExitStatus {
        let start = Instant::now();
        loop {
            if let Some(status) = poll_exit(child).unwrap() {
                return status;
            }
            assert!(start.elapsed() < Duration::from_secs(2));
            thread::sleep(Duration::from_millis(1));
        }
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
        let status = await_exit(&mut child);
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
    fn reserved_baseline_statistics_stays_null() {
        let report = serde_json::to_value(ExecutionReport::default()).unwrap();
        assert!(report["baseline_statistics"].is_null());
    }
    #[rstest::rstest]
    #[case(Reproduction::Replay, "replay")]
    #[case(Reproduction::Minimize, "minimize")]
    fn reproduction_modes_are_closed(#[case] mode: Reproduction, #[case] expected: &str) {
        assert_eq!(mode.report_mode(), expected);
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
    }
    #[test]
    fn discovery_rejects_empty_selection() {
        let empty: Discovery = serde_json::from_value(json!({"nodeids": []})).unwrap();
        assert!(empty.selection_complete());
        assert_eq!(
            empty.admit().unwrap_err().to_string(),
            "selection contains no supported Hypothesis tests"
        );
    }
    #[test]
    fn discovery_rejects_collection_failure() {
        let failed: Discovery =
            serde_json::from_value(json!({"nodeids": ["test"], "error": "collection failed"}))
                .unwrap();
        assert!(!failed.selection_complete());
        assert_eq!(
            failed.admit().unwrap_err().to_string(),
            "pytest collection failed; no targets executed: \"collection failed\""
        );
    }
    #[rstest::rstest]
    #[case(false, true, true, Some("campaign_cancelled"))]
    #[case(true, true, true, Some("interrupted"))]
    #[case(false, false, true, Some("case_timeout"))]
    fn precedence(
        #[case] interrupted: bool,
        #[case] campaign: bool,
        #[case] case: bool,
        #[case] expected: Option<&str>,
    ) {
        assert_eq!(expiry(interrupted, campaign, case), expected);
    }
    #[cfg(unix)]
    #[rstest::rstest]
    #[case::normal(libc::CLD_EXITED, 7, 7 << 8)]
    #[case::signal(libc::CLD_KILLED, libc::SIGTERM, libc::SIGTERM)]
    #[case::core_dump(libc::CLD_DUMPED, libc::SIGABRT, libc::SIGABRT | 0x80)]
    fn unix_notification_preserves_raw_status(
        #[case] code: i32,
        #[case] value: i32,
        #[case] raw: i32,
    ) {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(decode_unix_exit(code, value).unwrap().into_raw(), raw);
    }
    #[cfg(unix)]
    #[test]
    fn unexpected_unix_notification_is_rejected() {
        assert!(decode_unix_exit(libc::CLD_STOPPED, 0).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn interrupted_exit_poll_remains_pending() {
        assert!(
            polling_error(io::Error::from(io::ErrorKind::Interrupted))
                .unwrap()
                .is_none()
        );
    }
    #[cfg(unix)]
    #[test]
    fn failed_exit_poll_preserves_os_error() {
        assert_eq!(
            polling_error(io::Error::from_raw_os_error(libc::ECHILD))
                .unwrap_err()
                .raw_os_error(),
            Some(libc::ECHILD)
        );
    }
    #[cfg(unix)]
    #[test]
    fn polling_an_already_reaped_child_reports_error() {
        let mut child = Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .spawn()
            .unwrap();
        child.wait().unwrap();
        assert_eq!(
            poll_exit(&mut child).unwrap_err().raw_os_error(),
            Some(libc::ECHILD)
        );
    }
    #[cfg(unix)]
    proptest! {
        #[test]
        fn unix_exit_status_keeps_the_low_byte(value in any::<i32>()) {
            prop_assert_eq!(decode_unix_exit(libc::CLD_EXITED, value).unwrap().code(), Some(value & 0xff));
        }
    }
    #[rstest::rstest]
    #[case::pass("pass", false, Some("pass"))]
    #[case::reject("reject", false, Some("reject"))]
    #[case::failure("failure", false, Some("failure"))]
    #[case::oom("oom", false, Some("oom"))]
    #[case::unknown("unknown", false, None)]
    #[case::oom_overrides_pass("pass", true, Some("oom"))]
    #[case::oom_overrides_failure("failure", true, Some("oom"))]
    #[case::oom_overrides_unknown("unknown", true, Some("oom"))]
    fn observed_result_admission_preserves_diagnostics(
        #[case] kind: &str,
        #[case] oom: bool,
        #[case] expected: Option<&str>,
    ) {
        let observation = Observation {
            kind: kind.into(),
            rendered: Some("detail".into()),
            fingerprint: Some("identity".into()),
            replay_token: Some(vec![2, 7]),
            ..Observation::default()
        };
        let actual = admit_observation(observation, oom).map(|value| {
            (
                value.kind,
                value.rendered,
                value.fingerprint,
                value.replay_token,
            )
        });
        let expected = expected.map(|kind| {
            (
                kind.to_owned(),
                Some("detail".into()),
                Some("identity".into()),
                Some(vec![2, 7]),
            )
        });
        assert_eq!(actual, expected);
    }
    #[cfg(unix)]
    fn unread_command_stream_fixture() -> SessionFixture {
        let mut fixture = SessionFixture::new(&[]);
        fixture.config.limits.campaign_ms = Some(10_000);
        fixture.config.limits.case_ms = 2_000;
        let frames: Vec<Value> = std::iter::once(json!({"kind":"ready","environment":{}}))
            .chain((1..=512).flat_map(|case_id| [
                json!({"kind":"next","test_id":"test"}),
                json!({"kind":"result","case_id":case_id,"observation":{"nodeid":"test","kind":"pass"}}),
            ]))
            .collect();
        let mut bytes = Vec::new();
        frames
            .iter()
            .try_for_each(|frame| wire::write(&mut bytes, frame.clone()))
            .unwrap();
        let path = fixture.root.join("unacknowledged-frames");
        std::fs::write(&path, bytes).unwrap();
        std::fs::write(
            &fixture.config.python,
            format!("#!/bin/sh\ncat '{}'\nsleep 60\n", path.display()),
        )
        .unwrap();
        fixture
    }

    #[cfg(unix)]
    #[test]
    fn session_command_backpressure_is_bounded_infrastructure() {
        let fixture = unread_command_stream_fixture();
        let mut store =
            ArtifactStore::open_matching(&fixture.config.output, &fixture.config.identity).unwrap();
        let mut report = ExecutionReport::default();
        let result = Executor::new(Arc::new(AtomicBool::new(false)))
            .session(
                &fixture.config,
                &Invocation {
                    test_id: "test".into(),
                    raw_input: Some(vec![255; 4096]),
                },
                SessionOperation::Explore,
                Instant::now(),
                &mut store,
                &mut report,
                None,
            )
            .unwrap();
        let observation = result.observation.unwrap();
        assert_eq!(observation.kind, "infrastructure", "{observation:?}");
        assert_eq!(
            observation.rendered,
            Some(mpsc::TrySendError::Full(json!(null)).to_string()),
            "{observation:?}",
        );
        assert!(report.attempts > 0);
        assert!(report.attempts <= 512);
    }
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires delegated cgroup"]
    fn native_resource_counter_error_propagates_from_raw_crash_replay() {
        let mut fixture = SessionFixture::sequenced(&[
            vec![
                json!({"kind":"ready","environment":{}}),
                json!({"kind":"next","test_id":"test"}),
            ],
            vec![json!({"kind":"ready","environment":{}})],
        ]);
        let first = fixture.root.join("session-1");
        let script = std::fs::read_to_string(&first).unwrap();
        let acknowledged = script.strip_suffix("sleep 60\n").unwrap();
        std::fs::write(first, format!("{acknowledged}kill -KILL $$\n")).unwrap();
        let fault = NativeCgroupFault::install(&mut fixture, 2, "memory.events", false);
        let result =
            Executor::new(Arc::new(AtomicBool::new(false))).explore("test", fixture.config.clone());
        fault.cleanup().unwrap();
        assert_eq!(
            result.unwrap_err().to_string(),
            ExecutionError::Transport(io::Error::from_raw_os_error(libc::EACCES)).to_string()
        );
        let original: Value = serde_json::from_slice(
            &std::fs::read(fixture.config.output.join("finding-0.bin")).unwrap(),
        )
        .unwrap();
        assert_eq!(original["observation"]["kind"], "native_crash");
        assert!(original["observation"]["replay_token"].is_null());
    }
    #[rstest::rstest]
    #[case::explore(SessionOperation::Explore, "run", json!(null))]
    #[case::replay(SessionOperation::Replay(&[0, 255]), "replay", json!([0, 255]))]
    #[case::minimize(SessionOperation::Minimize(&[7]), "minimize", json!([7]))]
    #[case::raw_replay(SessionOperation::RawReplay, "run", json!(null))]
    #[case::empty_replay(SessionOperation::Replay(&[]), "replay", json!([]))]
    #[case::empty_minimize(SessionOperation::Minimize(&[]), "minimize", json!([]))]
    fn session_start_preserves_operation_and_opaque_token(
        #[case] operation: SessionOperation<'_>,
        #[case] mode: &str,
        #[case] token: Value,
    ) {
        assert_eq!(
            operation.start(&["-k".into(), "selected".into()], "test", 1.25),
            json!({"kind":"start","pytest_args":["-k","selected"],"test_id":"test","mode":mode,"token":token,"budget_seconds":1.25})
        );
    }
    proptest::proptest! {
        #[test]
        fn semantic_session_start_preserves_every_token_byte(token in proptest::collection::vec(any::<u8>(), 0..4097)) {
            let frame = SessionOperation::Replay(&token).start(&[], "test", 2.0);
            prop_assert_eq!(&frame["token"], &json!(token));
        }
    }
}
