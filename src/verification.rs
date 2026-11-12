//! Fresh reproduction evidence: identity matching and incomplete-check policy.
use crate::{
    artifacts::FindingObservation,
    executor::{Observation, OutcomeKind},
};

/// Identity captured before fresh execution, independent of its reported verdict.
/// Saved raw records retain the version-1 semantic-or-exact-detail matching rule;
/// automatic timeout confirmation compares the classification alone.
pub(crate) struct FailureIdentity<'a> {
    assertion: AssertionIdentity<'a>,
    raw: Option<RawIdentity<'a>>,
}
// Absent fingerprints remain comparable under the version-1 matching contract.
enum AssertionIdentity<'a> {
    Disabled,
    Fingerprint(Option<&'a str>),
}
enum RawIdentity<'a> {
    Kind(&'a OutcomeKind),
    KindAndDetail(&'a OutcomeKind, Option<&'a str>),
}
impl<'a> FailureIdentity<'a> {
    /// A semantic failure matches the assertion fingerprint, never rendered values.
    /// Two absent fingerprints compare equal under the version-1 contract.
    pub(crate) fn semantic(fingerprint: Option<&'a str>) -> Self {
        Self {
            assertion: AssertionIdentity::Fingerprint(fingerprint),
            raw: None,
        }
    }
    /// Capture an automatically discovered failure before fresh verification.
    /// Timeout confirmation matches kind alone; native crashes also match exit detail.
    pub(crate) fn observed(original: &'a Observation) -> Self {
        match original.kind {
            OutcomeKind::Failure => Self::semantic(original.fingerprint.as_deref()),
            OutcomeKind::CaseTimeout => Self {
                assertion: AssertionIdentity::Disabled,
                raw: Some(RawIdentity::Kind(&original.kind)),
            },
            _ => Self {
                assertion: AssertionIdentity::Disabled,
                raw: Some(RawIdentity::KindAndDetail(
                    &original.kind,
                    original.rendered.as_deref(),
                )),
            },
        }
    }
    /// Apply the saved-record compatibility contract for a standalone operation.
    /// Semantic matching includes absent-fingerprint equality. A record without a
    /// replay token also accepts exact kind/detail equality, including timeouts.
    pub(crate) fn saved(original: &'a FindingObservation) -> Self {
        Self {
            assertion: AssertionIdentity::Fingerprint(original.fingerprint.as_deref()),
            raw: original
                .replay_token
                .is_none()
                .then_some(RawIdentity::KindAndDetail(
                    &original.kind,
                    original.rendered.as_deref(),
                )),
        }
    }
    /// An incomplete check never proves a match or a mismatch. Skip and rejection
    /// are retained for standalone reporting; `candidate_status` treats them as unstable.
    pub(crate) fn check(&self, actual: Option<&Observation>) -> Verdict {
        let Some(actual) = actual else {
            return Verdict::Pending;
        };
        match actual.kind {
            OutcomeKind::CampaignCancelled
            | OutcomeKind::Interrupted
            | OutcomeKind::Infrastructure => return Verdict::Pending,
            OutcomeKind::Skip => return Verdict::Skipped,
            OutcomeKind::Reject => return Verdict::Rejected,
            _ => (),
        }
        let semantic = actual.kind == OutcomeKind::Failure
            && match self.assertion {
                AssertionIdentity::Disabled => false,
                AssertionIdentity::Fingerprint(expected) => {
                    expected == actual.fingerprint.as_deref()
                }
            };
        let raw = self.raw.as_ref().is_some_and(|identity| match identity {
            RawIdentity::Kind(kind) => *kind == &actual.kind,
            RawIdentity::KindAndDetail(kind, detail) => {
                *kind == &actual.kind && *detail == actual.rendered.as_deref()
            }
        });
        if semantic || raw {
            Verdict::Confirmed
        } else {
            Verdict::Unstable
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Pending,
    Confirmed,
    Unstable,
    Skipped,
    Rejected,
}
impl Verdict {
    #[must_use]
    pub fn status(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Confirmed => "confirmed",
            Self::Unstable => "unstable",
            Self::Skipped => "skipped",
            Self::Rejected => "rejected",
        }
    }
    /// A candidate is confirmed only by the same completed failure; an unusable
    /// test variant is an unstable candidate rather than a successful reduction.
    pub(crate) fn candidate_status(self) -> Self {
        match self {
            Self::Skipped | Self::Rejected => Self::Unstable,
            _ => self,
        }
    }
}

/// Verification evidence has either a closed verdict, no recorded check, or a
/// free-form diagnostic. Version-1 reports encode all three as plain strings.
#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(from = "String", into = "String")]
pub enum Verification {
    #[default]
    Empty,
    Verdict(Verdict),
    Diagnostic(String),
}
impl From<String> for Verification {
    fn from(label: String) -> Self {
        match label.as_str() {
            "" => Self::Empty,
            "pending" => Self::Verdict(Verdict::Pending),
            "confirmed" => Self::Verdict(Verdict::Confirmed),
            "unstable" => Self::Verdict(Verdict::Unstable),
            "skipped" => Self::Verdict(Verdict::Skipped),
            "rejected" => Self::Verdict(Verdict::Rejected),
            _ => Self::Diagnostic(label),
        }
    }
}
impl From<&str> for Verification {
    fn from(label: &str) -> Self {
        label.to_owned().into()
    }
}
impl From<Verdict> for Verification {
    fn from(verdict: Verdict) -> Self {
        Self::Verdict(verdict)
    }
}
impl From<Verification> for String {
    fn from(verification: Verification) -> Self {
        match verification {
            Verification::Empty => String::new(),
            Verification::Verdict(verdict) => verdict.status().into(),
            Verification::Diagnostic(message) => message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn observed(
        kind: OutcomeKind,
        fingerprint: Option<&str>,
        rendered: Option<&str>,
    ) -> Observation {
        Observation {
            kind,
            fingerprint: fingerprint.map(str::to_owned),
            rendered: rendered.map(str::to_owned),
            ..Observation::default()
        }
    }

    #[rstest::rstest]
    #[case::same_assertion(OutcomeKind::Failure, Some("assertion-a"), Verdict::Confirmed)]
    #[case::different_assertion(OutcomeKind::Failure, Some("assertion-b"), Verdict::Unstable)]
    #[case::passing(OutcomeKind::Pass, Some("assertion-a"), Verdict::Unstable)]
    #[case::skipped(OutcomeKind::Skip, None, Verdict::Skipped)]
    #[case::rejected(OutcomeKind::Reject, None, Verdict::Rejected)]
    #[case::cancelled(OutcomeKind::CampaignCancelled, None, Verdict::Pending)]
    #[case::interrupted(OutcomeKind::Interrupted, None, Verdict::Pending)]
    #[case::infrastructure(OutcomeKind::Infrastructure, None, Verdict::Pending)]
    fn semantic_confirmation_requires_completed_matching_failure(
        #[case] kind: OutcomeKind,
        #[case] fingerprint: Option<&str>,
        #[case] expected: Verdict,
    ) {
        let original = observed(OutcomeKind::Failure, Some("assertion-a"), None);
        assert_eq!(
            FailureIdentity::observed(&original).check(Some(&observed(kind, fingerprint, None))),
            expected
        );
    }

    #[test]
    fn missing_check_is_pending() {
        assert_eq!(
            FailureIdentity::semantic(Some("assertion-a")).check(None),
            Verdict::Pending
        );
    }

    #[rstest::rstest]
    #[case::skipped(Verdict::Skipped, "skipped", "unstable")]
    #[case::rejected(Verdict::Rejected, "rejected", "unstable")]
    #[case::pending(Verdict::Pending, "pending", "pending")]
    #[case::confirmed(Verdict::Confirmed, "confirmed", "confirmed")]
    #[case::unstable(Verdict::Unstable, "unstable", "unstable")]
    fn verdict_labels(#[case] verdict: Verdict, #[case] standalone: &str, #[case] candidate: &str) {
        assert_eq!(verdict.status(), standalone);
        assert_eq!(verdict.candidate_status().status(), candidate);
    }

    #[rstest::rstest]
    #[case::same_native_detail(
        OutcomeKind::NativeCrash,
        "exit-a",
        OutcomeKind::NativeCrash,
        "exit-a",
        Verdict::Confirmed
    )]
    #[case::different_native_detail(
        OutcomeKind::NativeCrash,
        "exit-a",
        OutcomeKind::NativeCrash,
        "exit-b",
        Verdict::Unstable
    )]
    #[case::different_kind(
        OutcomeKind::NativeCrash,
        "exit-a",
        OutcomeKind::CaseTimeout,
        "exit-a",
        Verdict::Unstable
    )]
    #[case::assertion_is_not_native(
        OutcomeKind::NativeCrash,
        "exit-a",
        OutcomeKind::Failure,
        "exit-a",
        Verdict::Unstable
    )]
    #[case::timeout_ignores_detail(
        OutcomeKind::CaseTimeout,
        "first",
        OutcomeKind::CaseTimeout,
        "second",
        Verdict::Confirmed
    )]
    fn observed_native_identity(
        #[case] kind: OutcomeKind,
        #[case] detail: &str,
        #[case] actual_kind: OutcomeKind,
        #[case] actual_detail: &str,
        #[case] expected: Verdict,
    ) {
        let original = observed(kind, None, Some(detail));
        assert_eq!(
            FailureIdentity::observed(&original).check(Some(&observed(
                actual_kind,
                None,
                Some(actual_detail)
            ))),
            expected
        );
    }

    #[rstest::rstest]
    #[case::unknown_saved_kind(OutcomeKind::Other("future-result".into()), None, None, "same", (OutcomeKind::Other("future-result".into()), None, Some("same")), Verdict::Confirmed)]
    #[case::different_unknown_saved_kind(OutcomeKind::Other("future-result".into()), None, None, "same", (OutcomeKind::Other("another-result".into()), None, Some("same")), Verdict::Unstable)]
    #[case::unknown_detail_differs(OutcomeKind::Other("future-result".into()), None, None, "first", (OutcomeKind::Other("future-result".into()), None, Some("second")), Verdict::Unstable)]
    #[case::empty_saved_kind(OutcomeKind::Other(String::new()), None, None, "same", (OutcomeKind::Other(String::new()), None, Some("same")), Verdict::Confirmed)]
    #[case::semantic_different_fingerprint(OutcomeKind::Failure, Some("assertion-a"), Some(vec![1]), "first", (OutcomeKind::Failure, Some("assertion-b"), Some("first")), Verdict::Unstable)]
    #[case::semantic_ignores_detail(OutcomeKind::Failure, Some("assertion-a"), Some(vec![1]), "first", (OutcomeKind::Failure, Some("assertion-a"), Some("different")), Verdict::Confirmed)]
    #[case::semantic_rejects_native(OutcomeKind::Failure, Some("assertion-a"), Some(vec![1]), "first", (OutcomeKind::NativeCrash, Some("assertion-a"), Some("first")), Verdict::Unstable)]
    #[case::raw_semantic_alternative(OutcomeKind::NativeCrash, Some("recorded"), None, "exit-a", (OutcomeKind::Failure, Some("recorded"), None), Verdict::Confirmed)]
    #[case::raw_different_fingerprint(OutcomeKind::NativeCrash, Some("recorded"), None, "exit-a", (OutcomeKind::Failure, Some("different"), None), Verdict::Unstable)]
    #[case::raw_exact_detail(OutcomeKind::NativeCrash, Some("recorded"), None, "exit-a", (OutcomeKind::NativeCrash, None, Some("exit-a")), Verdict::Confirmed)]
    #[case::absent_fingerprints(OutcomeKind::NativeCrash, None, None, "exit-a", (OutcomeKind::Failure, None, None), Verdict::Confirmed)]
    #[case::saved_timeout_detail_differs(OutcomeKind::CaseTimeout, None, None, "first", (OutcomeKind::CaseTimeout, None, Some("second")), Verdict::Unstable)]
    #[case::saved_timeout_detail_matches(OutcomeKind::CaseTimeout, None, None, "first", (OutcomeKind::CaseTimeout, None, Some("first")), Verdict::Confirmed)]
    #[case::saved_rejection(OutcomeKind::CaseTimeout, None, None, "first", (OutcomeKind::Reject, None, None), Verdict::Rejected)]
    fn saved_identity_compatibility(
        #[case] kind: OutcomeKind,
        #[case] fingerprint: Option<&str>,
        #[case] replay_token: Option<Vec<u8>>,
        #[case] detail: &str,
        #[case] actual: (OutcomeKind, Option<&str>, Option<&str>),
        #[case] expected: Verdict,
    ) {
        let saved = crate::artifacts::FindingObservation {
            kind,
            fingerprint: fingerprint.map(str::to_owned),
            replay_token,
            rendered: Some(detail.into()),
        };
        assert_eq!(
            FailureIdentity::saved(&saved).check(Some(&observed(actual.0, actual.1, actual.2))),
            expected
        );
    }

    #[test]
    fn observed_absent_fingerprints_match() {
        let original = observed(OutcomeKind::Failure, None, None);
        assert_eq!(
            FailureIdentity::observed(&original).check(Some(&original)),
            Verdict::Confirmed
        );
    }

    proptest! {
        #[test]
        fn semantic_confirmation_ignores_rendered_values(fingerprint in prop::option::of(".{0,80}"), first in prop::option::of(".{0,80}"), second in prop::option::of(".{0,80}")) {
            let original = observed(OutcomeKind::Failure, fingerprint.as_deref(), first.as_deref());
            let actual = observed(OutcomeKind::Failure, fingerprint.as_deref(), second.as_deref());
            prop_assert_eq!(FailureIdentity::observed(&original).check(Some(&actual)), Verdict::Confirmed);
        }
        #[test]
        fn distinct_native_exit_details_cannot_confirm(detail in ".{0,80}", suffix in ".{1,40}") {
            let changed = format!("{detail}{suffix}");
            let original = observed(OutcomeKind::NativeCrash, None, Some(&detail));
            let actual = observed(OutcomeKind::NativeCrash, None, Some(&changed));
            prop_assert_eq!(FailureIdentity::observed(&original).check(Some(&actual)), Verdict::Unstable);
        }
    }
}
