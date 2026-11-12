//! Product command transaction: discovery, admission, execution and presentation.
use crate::{
    artifacts::ReplayContext,
    campaign::{self, CampaignConfig, TargetStatus},
    executor::{ExecutionSettings, Executor, Limits, Reproduction},
};
use serde_json::{Value, json};
use std::{
    env, io,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};
/// The user operation; reproduction requires a saved finding path.
pub enum Operation {
    Run,
    Replay(PathBuf),
    Minimize(PathBuf),
}
impl Operation {
    fn name(&self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Replay(_) => "replay",
            Self::Minimize(_) => "minimize",
        }
    }
}
// Pair reproduction kind with its admitted context so execution cannot lose
// the writer lock or construct a reproduction without a finding.
enum PreparedOperation {
    Run,
    Reproduce {
        context: Box<ReplayContext>,
        kind: Reproduction,
    },
}
/// User choices independent of the adapter wire protocol and worker modes.
pub struct Request {
    pub operation: Operation,
    pub python: PathBuf,
    pub selection: Vec<String>,
    pub limits: Limits,
    pub seed: u64,
    pub store: PathBuf,
}
/// A completed command retains its replay writer lock through report publication.
pub struct CommandOutcome {
    report: Value,
    _operation: PreparedOperation,
}
impl CommandOutcome {
    /// Publish the result before releasing any finding transaction lock.
    /// # Errors
    /// Returns serialization or output persistence errors.
    pub fn publish(self, output: Option<&Path>) -> io::Result<i32> {
        crate::report::publish_output(output, &self.report)?;
        Ok(crate::report::exit_code(&self.report))
    }
}
/// Execute a product command, retaining partial context for early error reports.
/// Collection consumes the same overall budget as execution. No target is admitted
/// until discovery succeeds and saved finding compatibility has been checked.
/// # Errors
/// Returns discovery, storage, compatibility or execution failures.
#[allow(clippy::too_many_lines)]
pub fn execute(
    request: Request,
    context: &mut Value,
    flag: Arc<AtomicBool>,
) -> Result<CommandOutcome, Box<dyn std::error::Error>> {
    let Request {
        operation,
        python,
        mut selection,
        mut limits,
        seed,
        store,
    } = request;
    let command = operation.name();
    let operation = match operation {
        Operation::Run => PreparedOperation::Run,
        Operation::Replay(path) => PreparedOperation::Reproduce {
            context: Box::new(ReplayContext::open(&path)?),
            kind: Reproduction::Replay,
        },
        Operation::Minimize(path) => PreparedOperation::Reproduce {
            context: Box::new(ReplayContext::open(&path)?),
            kind: Reproduction::Minimize,
        },
    };
    if selection.is_empty()
        && let PreparedOperation::Reproduce { context, .. } = &operation
    {
        selection = context.selection().to_vec();
    }
    let requested_limits = serde_json::to_value(&limits)?;
    context["effective_limits"] = requested_limits.clone();
    context["selection"] = json!(selection);
    let overall_start = std::time::Instant::now();
    let discovered = crate::executor::collect(&python, &selection, &limits, &flag)?;
    if let Some(budget) = limits.campaign_ms.as_mut() {
        *budget = budget
            .saturating_sub(u64::try_from(overall_start.elapsed().as_millis()).unwrap_or(u64::MAX));
    }
    context["environment"] = discovered.environment().clone();
    context["targets"] = json!(
        discovered
            .targets()
            .iter()
            .map(|target| json!({"target":target,"status":TargetStatus::Unexecuted}))
            .collect::<Vec<_>>()
    );
    context["selection_complete"] = json!(discovered.selection_complete());
    discovered.admit()?;
    let targets = discovered.targets().to_vec();
    let identity = discovered.identity(&selection);
    context["environment_identity"] = identity.clone();
    let environment_identity = identity.clone();
    let remaining_execution_limits = serde_json::to_value(&limits)?;
    let replay_python = python.clone();
    let mut report = match &operation {
        PreparedOperation::Run => campaign::run(
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
        )?,
        PreparedOperation::Reproduce {
            context: replay,
            kind,
        } => {
            replay.check_environment(&identity)?;
            let config = ExecutionSettings {
                python,
                pytest_args: selection,
                limits,
                seed,
                output: replay.directory().to_owned(),
                identity: replay.store_identity().clone(),
            };
            serde_json::to_value(Executor::new(flag).operation(
                &config,
                replay.original(),
                *kind,
            )?)?
        }
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
    crate::report::present_findings(
        &mut report,
        &env::current_exe()?,
        &replay_python,
        best_effort,
    )?;
    Ok(CommandOutcome {
        report,
        _operation: operation,
    })
}
