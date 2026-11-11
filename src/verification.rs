//! Fresh reproduction evidence: identity matching and incomplete-check policy.
use crate::{artifacts::FindingObservation, executor::Observation};

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
    Kind(&'a str),
    KindAndDetail(&'a str, Option<&'a str>),
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
        match original.kind.as_str() {
            "failure" => Self::semantic(original.fingerprint.as_deref()),
            "case_timeout" => Self {
                assertion: AssertionIdentity::Disabled,
                raw: Some(RawIdentity::Kind("case_timeout")),
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
        match actual.kind.as_str() {
            "campaign_cancelled" | "interrupted" | "infrastructure" => return Verdict::Pending,
            "skip" => return Verdict::Skipped,
            "reject" => return Verdict::Rejected,
            _ => (),
        }
        let semantic = actual.kind == "failure"
            && match self.assertion {
                AssertionIdentity::Disabled => false,
                AssertionIdentity::Fingerprint(expected) => {
                    expected == actual.fingerprint.as_deref()
                }
            };
        let raw = self.raw.as_ref().is_some_and(|identity| match identity {
            RawIdentity::Kind(kind) => *kind == actual.kind,
            RawIdentity::KindAndDetail(kind, detail) => {
                *kind == actual.kind && *detail == actual.rendered.as_deref()
            }
        });
        if semantic || raw {
            Verdict::Confirmed
        } else {
            Verdict::Unstable
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Verdict {
    Pending,
    Confirmed,
    Unstable,
    Skipped,
    Rejected,
}
impl Verdict {
    pub(crate) fn status(self) -> &'static str {
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
    pub(crate) fn candidate_status(self) -> &'static str {
        match self {
            Self::Skipped | Self::Rejected => "unstable",
            _ => self.status(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn observed(kind: &str, fingerprint: Option<&str>, rendered: Option<&str>) -> Observation {
        Observation {
            kind: kind.into(),
            fingerprint: fingerprint.map(str::to_owned),
            rendered: rendered.map(str::to_owned),
            ..Observation::default()
        }
    }

    #[rstest::rstest]
    #[case::same_assertion("failure", Some("assertion-a"), Verdict::Confirmed)]
    #[case::different_assertion("failure", Some("assertion-b"), Verdict::Unstable)]
    #[case::passing("pass", Some("assertion-a"), Verdict::Unstable)]
    #[case::skipped("skip", None, Verdict::Skipped)]
    #[case::rejected("reject", None, Verdict::Rejected)]
    #[case::cancelled("campaign_cancelled", None, Verdict::Pending)]
    #[case::interrupted("interrupted", None, Verdict::Pending)]
    #[case::infrastructure("infrastructure", None, Verdict::Pending)]
    fn semantic_confirmation_requires_completed_matching_failure(
        #[case] kind: &str,
        #[case] fingerprint: Option<&str>,
        #[case] expected: Verdict,
    ) {
        let original = observed("failure", Some("assertion-a"), None);
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
        assert_eq!(verdict.candidate_status(), candidate);
    }

    #[rstest::rstest]
    #[case::same_native_detail(
        "native_crash",
        "exit-a",
        "native_crash",
        "exit-a",
        Verdict::Confirmed
    )]
    #[case::different_native_detail(
        "native_crash",
        "exit-a",
        "native_crash",
        "exit-b",
        Verdict::Unstable
    )]
    #[case::different_kind("native_crash", "exit-a", "case_timeout", "exit-a", Verdict::Unstable)]
    #[case::assertion_is_not_native(
        "native_crash",
        "exit-a",
        "failure",
        "exit-a",
        Verdict::Unstable
    )]
    #[case::timeout_ignores_detail(
        "case_timeout",
        "first",
        "case_timeout",
        "second",
        Verdict::Confirmed
    )]
    fn observed_native_identity(
        #[case] kind: &str,
        #[case] detail: &str,
        #[case] actual_kind: &str,
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
    #[case::semantic_different_fingerprint("failure", Some("assertion-a"), Some(vec![1]), "first", ("failure", Some("assertion-b"), Some("first")), Verdict::Unstable)]
    #[case::semantic_ignores_detail("failure", Some("assertion-a"), Some(vec![1]), "first", ("failure", Some("assertion-a"), Some("different")), Verdict::Confirmed)]
    #[case::semantic_rejects_native("failure", Some("assertion-a"), Some(vec![1]), "first", ("native_crash", Some("assertion-a"), Some("first")), Verdict::Unstable)]
    #[case::raw_semantic_alternative("native_crash", Some("recorded"), None, "exit-a", ("failure", Some("recorded"), None), Verdict::Confirmed)]
    #[case::raw_different_fingerprint("native_crash", Some("recorded"), None, "exit-a", ("failure", Some("different"), None), Verdict::Unstable)]
    #[case::raw_exact_detail("native_crash", Some("recorded"), None, "exit-a", ("native_crash", None, Some("exit-a")), Verdict::Confirmed)]
    #[case::absent_fingerprints("native_crash", None, None, "exit-a", ("failure", None, None), Verdict::Confirmed)]
    #[case::saved_timeout_detail_differs("case_timeout", None, None, "first", ("case_timeout", None, Some("second")), Verdict::Unstable)]
    #[case::saved_timeout_detail_matches("case_timeout", None, None, "first", ("case_timeout", None, Some("first")), Verdict::Confirmed)]
    #[case::saved_rejection("case_timeout", None, None, "first", ("reject", None, None), Verdict::Rejected)]
    fn saved_identity_compatibility(
        #[case] kind: &str,
        #[case] fingerprint: Option<&str>,
        #[case] replay_token: Option<Vec<u8>>,
        #[case] detail: &str,
        #[case] actual: (&str, Option<&str>, Option<&str>),
        #[case] expected: Verdict,
    ) {
        let saved = crate::artifacts::FindingObservation {
            kind: kind.into(),
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
        let original = observed("failure", None, None);
        assert_eq!(
            FailureIdentity::observed(&original).check(Some(&original)),
            Verdict::Confirmed
        );
    }

    proptest! {
        #[test]
        fn semantic_confirmation_ignores_rendered_values(fingerprint in prop::option::of(".{0,80}"), first in prop::option::of(".{0,80}"), second in prop::option::of(".{0,80}")) {
            let original = observed("failure", fingerprint.as_deref(), first.as_deref());
            let actual = observed("failure", fingerprint.as_deref(), second.as_deref());
            prop_assert_eq!(FailureIdentity::observed(&original).check(Some(&actual)), Verdict::Confirmed);
        }
        #[test]
        fn distinct_native_exit_details_cannot_confirm(detail in ".{0,80}", suffix in ".{1,40}") {
            let changed = format!("{detail}{suffix}");
            let original = observed("native_crash", None, Some(&detail));
            let actual = observed("native_crash", None, Some(&changed));
            prop_assert_eq!(FailureIdentity::observed(&original).check(Some(&actual)), Verdict::Unstable);
        }
    }
}
