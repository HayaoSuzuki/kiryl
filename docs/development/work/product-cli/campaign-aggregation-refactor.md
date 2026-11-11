# Typed campaign progress and bounded log merging

This follow-up examines campaign aggregation, beyond the earlier storage/status
refactor. The problem was runtime JSON serving as both mutable progress and
serialized output: every slice read counters and log bytes back from JSON with
fallback defaults. The same truncated-stream accounting was implemented in the
campaign and worker-session paths.

## Decision and compatibility

TargetProgress now owns cumulative counters, a bounded retained stream and one
latest-slice snapshot. TargetStatus and SliceSnapshot encode supported states;
serialization preserves the existing public field names and optional-field shape.
An infrastructure error replaces the snapshot while preserving accumulated data.
The scheduler supplies cumulative charge rather than a delta.

LogTail owns both append and merge. Its internal fields are private; merging
counts the source's full observed stream, even when its retained bytes were
truncated. Counter saturation and retention are enforced in one implementation.

The alternative was extracting JSON getters or a generic aggregation framework.
Neither would hide the representation or reduce the concepts callers need. The
selected private types add no product configuration or public progress API.
The unpublished Rust LogTail API is intentionally narrower; CLI options, JSON,
exit statuses, scheduling selection and storage formats are unchanged. No user
command update is needed.

## Five design review passes

1. Purpose: campaign owns per-target progress, executor owns bounded stream mechanics.
2. Information hiding: progress accumulation no longer reads serialized counters.
3. Depth: one record operation handles counters, logs, terminal status and snapshot.
4. Scope: choose_next, fair-time charging and worker/resource semantics stay intact.
5. Comments: cumulative charge, stop return value and error preservation are explicit.

## Five plan review passes

1. Restrict change to in-memory representation and duplicate stream merging.
2. Freeze unexecuted/success/error JSON shapes with focused assertions.
3. Keep cumulative scheduler accounting separate from per-slice measurements.
4. Check zero retention, truncation and saturating totals with a raw-stream oracle.
5. Reuse the existing product/native suites rather than adding configuration modes.

## Five implementation review passes

1. Optional accumulated state omits fields for unexecuted and pre-first-slice errors.
2. Latest report stays distinct from cumulative totals, avoiding slice-local reporting.
3. Error transitions retain attempts, counters, charge and output.
4. Merge uses observed counts, not retained byte lengths, for source accounting.
5. Both worker and campaign call the same bounded merging implementation.

## Five test review passes

1. The progress regression first failed with the missing TargetProgress implementation.
2. Exact JSON assertions cover unexecuted shape and infrastructure snapshot shape.
3. Two slices demonstrate cumulative totals versus latest-slice attempts.
4. Explicit tests cover truncated inputs, zero capacity and saturation; proptest
   compares composed bounded merges against unbounded raw concatenation.
5. Full Rust, strict Clippy and product Python/CLI checks validate integration;
   native CI results belong to the actual PR revision on which they execute.

## Independent review and scoring

The first independent review found no critical/important behavior regression but
scored the changed campaign scope 7.5/10: interface promises and newcomer-facing
boundary documentation failed. The corrective actions were documenting the
transition guarantees and naming the argument cumulative_charged_ms.

The follow-up review confirmed both fixes and scored this campaign aggregation
scope 10/10, all eight diagnostics passing, with no remaining failed rows. This
is not a repository-wide score or a claim that no further improvement is possible.
The reviewer inspected code; execution results are verified by the coordinator.
