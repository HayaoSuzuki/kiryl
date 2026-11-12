// Copyright (c) 2026 kiryl contributors
//! Experiment-local state labels; string encoding belongs only to this boundary.
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutcomeKind {
    Pass,
    Reject,
    Skip,
    Failure,
    Oom,
    Infrastructure,
    NativeCrash,
    CaseTimeout,
    CampaignCancelled,
    Interrupted,
    Other(String),
}
impl Default for OutcomeKind {
    fn default() -> Self {
        Self::Other(String::new())
    }
}
impl Serialize for OutcomeKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Pass => "pass",
            Self::Reject => "reject",
            Self::Skip => "skip",
            Self::Failure => "failure",
            Self::Oom => "oom",
            Self::Infrastructure => "infrastructure",
            Self::NativeCrash => "native_crash",
            Self::CaseTimeout => "case_timeout",
            Self::CampaignCancelled => "campaign_cancelled",
            Self::Interrupted => "interrupted",
            Self::Other(value) => value,
        })
    }
}
impl<'de> Deserialize<'de> for OutcomeKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "pass" => Self::Pass,
            "reject" => Self::Reject,
            "skip" => Self::Skip,
            "failure" => Self::Failure,
            "oom" => Self::Oom,
            "infrastructure" => Self::Infrastructure,
            "native_crash" => Self::NativeCrash,
            "case_timeout" => Self::CaseTimeout,
            "campaign_cancelled" => Self::CampaignCancelled,
            "interrupted" => Self::Interrupted,
            _ => Self::Other(value),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verification {
    Pending,
    Confirmed,
    Unstable,
    Diagnostic(String),
}
impl Default for Verification {
    fn default() -> Self {
        Self::Diagnostic(String::new())
    }
}
impl Serialize for Verification {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Pending => "pending",
            Self::Confirmed => "confirmed",
            Self::Unstable => "unstable",
            Self::Diagnostic(value) => value,
        })
    }
}
impl<'de> Deserialize<'de> for Verification {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "pending" => Self::Pending,
            "confirmed" => Self::Confirmed,
            "unstable" => Self::Unstable,
            _ => Self::Diagnostic(value),
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportMode {
    #[default]
    Fixed,
    Guided,
    Unguided,
    Hypothesis,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum BackendName {
    #[serde(rename = "portable")]
    Portable,
    #[serde(rename = "cgroup-v2")]
    CgroupV2,
    #[serde(rename = "job-object")]
    JobObject,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IncomingKind {
    Ready,
    Next,
    ReplayCase,
    BaselineCase,
    BaselineDone,
    Skipped,
    Result,
    Candidate,
    Shrunk,
    Done,
    Error,
}

#[derive(Clone, Copy, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum OutgoingFrame<'a> {
    Start {
        pytest_args: &'a [String],
        test_id: &'a str,
        mode: WorkerMode,
        token: Option<&'a [u8]>,
        budget_seconds: f64,
    },
    End,
    Ack,
    Case {
        case_id: u64,
        input: &'a [u8],
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkerMode {
    Run,
    Replay,
    Minimize,
    Baseline,
}

pub(crate) fn decode_kind(message: &serde_json::Value) -> Option<IncomingKind> {
    message["kind"]
        .as_str()
        .and_then(|label| serde_json::from_value(serde_json::json!(label)).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn incoming_ready_string_is_admitted() {
        assert_eq!(
            decode_kind(&json!({"kind":"ready"})),
            Some(IncomingKind::Ready)
        );
    }
    #[test]
    fn incoming_object_kind_is_rejected() {
        assert_eq!(decode_kind(&json!({"kind":{"ready":null}})), None);
    }
    #[test]
    fn incoming_unknown_string_is_rejected() {
        assert_eq!(decode_kind(&json!({"kind":"future"})), None);
    }
    #[test]
    fn start_preserves_explicit_null_token() {
        let frame = OutgoingFrame::Start {
            pytest_args: &[],
            test_id: "node",
            mode: WorkerMode::Run,
            token: None,
            budget_seconds: 1.0,
        };
        assert_eq!(
            serde_json::to_value(frame).unwrap(),
            json!({"kind":"start", "pytest_args":[], "test_id":"node", "mode":"run", "token":null,"budget_seconds":1.0})
        );
    }
}
