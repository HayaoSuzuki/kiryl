-- Copyright (c) 2026 kiryl contributors
import Std

set_option maxHeartbeats 10000

namespace ResourceModel
structure State where
  consumed : Bool := false
  attempts : Nat := 0
  deriving DecidableEq

-- Consume authority before calling an OS operation, including when it fails.
def request (s : State) : State :=
  if s.consumed then s else { consumed := true, attempts := s.attempts + 1 }

def requests : Nat → State
  | 0 => {}
  | n + 1 => request (requests n)

theorem positive (n : Nat) : requests (n + 1) = { consumed := true, attempts := 1 } := by
  induction n with
  | zero => rfl
  | succ n ih =>
    change request (requests (n + 1)) = _
    rw [ih]
    rfl

theorem attempts_bounded (n : Nat) : (requests n).attempts ≤ 1 := by
  cases n with
  | zero => decide
  | succ n => rw [positive]; decide

-- A retry-on-error implementation would violate this on two requests.
example : (requests 2).attempts ≠ 2 := by decide
end ResourceModel
