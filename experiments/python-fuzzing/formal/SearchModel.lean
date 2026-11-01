-- Copyright (c) 2026 kiryl contributors
import Std

set_option maxHeartbeats 10000

namespace SearchModel
structure State where
  seen : List Nat := []
  entries : Nat := 1
  deriving DecidableEq

-- Count abstraction assumes fresh, bounded canonical payloads for new features.
def observe (s : State) (valid : Bool) (feature : Nat) : State :=
  if valid && !(s.seen.contains feature) then
    { seen := feature :: s.seen, entries := min 256 (s.entries + 1) }
  else s

theorem rejects_unchanged (s : State) (feature : Nat) : observe s false feature = s := by
  simp [observe]

theorem observe_bounded (s : State) (valid : Bool) (feature : Nat) (h : s.entries ≤ 256) :
    (observe s valid feature).entries ≤ 256 := by
  unfold observe
  split
  · exact Nat.min_le_left _ _
  · exact h

def run (s : State) (cases : List (Bool × Nat)) : State :=
  cases.foldl (fun state entry => observe state entry.1 entry.2) s

theorem all_sequences_bounded (s : State) (cases : List (Bool × Nat)) (h : s.entries ≤ 256) :
    (run s cases).entries ≤ 256 := by
  induction cases generalizing s with
  | nil => exact h
  | cons entry rest ih => exact ih (observe s entry.1 entry.2) (observe_bounded s _ _ h)

end SearchModel
