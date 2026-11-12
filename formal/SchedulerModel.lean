import Std

set_option maxHeartbeats 10000
namespace SchedulerModel

-- Stable minimum among unfinished targets. Charges model actual elapsed debt.
def step (best : Option (Nat × Nat)) (entry : (Nat × Bool) × Nat) : Option (Nat × Nat) :=
  if entry.1.2 then best
  else match best with
    | none => some (entry.1.1, entry.2)
    | some old => if entry.1.1 < old.1 then some (entry.1.1, entry.2) else best

def choose (targets : List (Nat × Bool)) : Option Nat :=
  ((targets.zipIdx).foldl step none).map Prod.snd

example : choose [(7, false), (1, false), (0, true)] = some 1 := by decide
example : choose [(1, false), (1, false)] = some 0 := by decide
example : choose [(0, true), (0, true)] = none := by decide
-- Sensitivity to ignoring overshoot or ignoring stopped status.
example : choose [(7, false), (1, false)] ≠ some 0 := by decide
example : choose [(0, true), (1, false)] ≠ some 0 := by decide

def admit (remaining : Nat) (interrupted : Bool) : Bool :=
  remaining > 0 && !interrupted
example : admit 0 false = false := by decide
example : admit 1 true = false := by decide
example : admit 1 false = true := by decide

structure Finding where
  original : List Nat
  derivatives : List (List Nat)

def derive (finding : Finding) (candidate : List Nat) : Finding :=
  { finding with derivatives := candidate :: finding.derivatives }

theorem derive_preserves_original (finding : Finding) (candidate : List Nat) :
    (derive finding candidate).original = finding.original := by rfl

end SchedulerModel
