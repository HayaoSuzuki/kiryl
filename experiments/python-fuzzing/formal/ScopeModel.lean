import Std

namespace ScopeModel

structure Scope where
  starts : Nat := 0
  finishes : Nat := 0
  active : Bool := false
  deriving Repr, DecidableEq

def invoke (s : Scope) : Scope :=
  { starts := s.starts + 1, finishes := s.finishes + 1, active := false }

def replay : Nat → Scope
  | 0 => {}
  | n + 1 => invoke (replay n)

-- A returning invocation must have completed both lifecycle transitions.
example : replay 1 = { starts := 1, finishes := 1, active := false } := by decide

set_option maxHeartbeats 10000 in
theorem replay_counts (n : Nat) :
    replay n = { starts := n, finishes := n, active := false } := by
  induction n with
  | zero => rfl
  | succ n ih => simp [replay, invoke, ih]

theorem normal_returns_balance (n : Nat) : (replay n).starts = (replay n).finishes := by
  rw [replay_counts]

-- Sensitivity witnesses, confined to the model. Minimal one-invocation
-- transaction loss, duplicate teardown, and zero/one boundary mistakes.
def lostFinish : Scope := { starts := 1, finishes := 0 }
def duplicateFinish : Scope := { starts := 1, finishes := 2 }
example : lostFinish.starts ≠ lostFinish.finishes := by decide
example : duplicateFinish.starts ≠ duplicateFinish.finishes := by decide
example : replay 0 ≠ replay 1 := by decide

end ScopeModel
