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
    use crate::executor::Observation;

    fn observed(kind: &str, fingerprint: Option<&str>, rendered: Option<&str>) -> Observation {
        Observation {
            kind: kind.into(),
            fingerprint: fingerprint.map(str::to_owned),
            rendered: rendered.map(str::to_owned),
            ..Observation::default()
        }
    }

    #[test]
    fn semantic_confirmation_requires_matching_failure_and_completed_check() {
        let original = observed("failure", Some("assertion-a"), None);
        let identity = FailureIdentity::observed(&original);
        for (kind, fingerprint, expected) in [
            ("failure", Some("assertion-a"), Verdict::Confirmed),
            ("failure", Some("assertion-b"), Verdict::Unstable),
            ("pass", Some("assertion-a"), Verdict::Unstable),
            ("skip", None, Verdict::Skipped),
            ("reject", None, Verdict::Rejected),
            ("campaign_cancelled", None, Verdict::Pending),
            ("interrupted", None, Verdict::Pending),
            ("infrastructure", None, Verdict::Pending),
        ] {
            assert_eq!(
                identity.check(Some(&observed(kind, fingerprint, None))),
                expected
            );
        }
        assert_eq!(identity.check(None), Verdict::Pending);
        assert_eq!(Verdict::Skipped.candidate_status(), "unstable");
        assert_eq!(Verdict::Rejected.candidate_status(), "unstable");
        assert_eq!(Verdict::Skipped.status(), "skipped");
    }

    #[test]
    fn native_identity_distinguishes_exit_detail_but_automatic_timeout_does_not() {
        let crash = observed("native_crash", None, Some("exit-a"));
        assert_eq!(
            FailureIdentity::observed(&crash).check(Some(&crash)),
            Verdict::Confirmed
        );
        assert_eq!(
            FailureIdentity::observed(&crash).check(Some(&observed(
                "native_crash",
                None,
                Some("exit-b")
            ))),
            Verdict::Unstable
        );
        assert_eq!(
            FailureIdentity::observed(&crash).check(Some(&observed(
                "case_timeout",
                None,
                Some("exit-a")
            ))),
            Verdict::Unstable
        );
        let timeout = observed("case_timeout", None, Some("first"));
        assert_eq!(
            FailureIdentity::observed(&timeout).check(Some(&observed(
                "case_timeout",
                None,
                Some("second")
            ))),
            Verdict::Confirmed
        );
    }

    #[test]
    fn saved_semantic_identity_does_not_accept_matching_native_details() {
        let saved = crate::artifacts::FindingObservation {
            kind: "failure".into(),
            fingerprint: Some("assertion-a".into()),
            replay_token: Some(vec![1]),
            rendered: Some("first".into()),
        };
        let identity = FailureIdentity::saved(&saved);
        assert_eq!(
            identity.check(Some(&observed(
                "failure",
                Some("assertion-b"),
                Some("first")
            ))),
            Verdict::Unstable
        );
        assert_eq!(
            identity.check(Some(&observed(
                "failure",
                Some("assertion-a"),
                Some("different")
            ))),
            Verdict::Confirmed
        );
        assert_eq!(
            identity.check(Some(&observed(
                "native_crash",
                Some("assertion-a"),
                Some("first")
            ))),
            Verdict::Unstable
        );
    }

    #[test]
    fn saved_raw_compatibility_retains_semantic_alternative() {
        let saved = crate::artifacts::FindingObservation {
            kind: "native_crash".into(),
            fingerprint: Some("recorded".into()),
            replay_token: None,
            rendered: Some("exit-a".into()),
        };
        let identity = FailureIdentity::saved(&saved);
        assert_eq!(
            identity.check(Some(&observed("failure", Some("recorded"), None))),
            Verdict::Confirmed
        );
        assert_eq!(
            identity.check(Some(&observed("failure", Some("different"), None))),
            Verdict::Unstable
        );
        assert_eq!(
            identity.check(Some(&observed("native_crash", None, Some("exit-a")))),
            Verdict::Confirmed
        );
        let without_fingerprint = crate::artifacts::FindingObservation {
            fingerprint: None,
            ..saved
        };
        assert_eq!(
            FailureIdentity::saved(&without_fingerprint)
                .check(Some(&observed("failure", None, None))),
            Verdict::Confirmed
        );
    }

    #[test]
    fn saved_raw_identity_preserves_detail_and_standalone_labels() {
        let original = crate::artifacts::FindingObservation {
            kind: "case_timeout".into(),
            fingerprint: None,
            replay_token: None,
            rendered: Some("first".into()),
        };
        let identity = FailureIdentity::saved(&original);
        assert_eq!(
            identity.check(Some(&observed("case_timeout", None, Some("second")))),
            Verdict::Unstable
        );
        assert_eq!(
            identity.check(Some(&observed("case_timeout", None, Some("first")))),
            Verdict::Confirmed
        );
        assert_eq!(
            identity
                .check(Some(&observed("reject", None, None)))
                .status(),
            "rejected"
        );
        let semantic = observed("failure", None, None);
        assert_eq!(
            FailureIdentity::observed(&semantic).check(Some(&semantic)),
            Verdict::Confirmed
        );
    }
}
