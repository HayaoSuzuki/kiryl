-- Copyright (c) 2026 kiryl contributors
import Std

set_option maxHeartbeats 10000

namespace BudgetModel

inductive Stop where
  | running | interrupted | campaign | caseTimeout
  deriving DecidableEq, Repr

def expiry (interrupt campaign caseExpired : Bool) : Stop :=
  if interrupt then .interrupted else if campaign then .campaign
  else if caseExpired then .caseTimeout else .running

theorem campaign_precedence (caseExpired : Bool) :
    expiry false true caseExpired = .campaign := by simp [expiry]

theorem interrupt_precedence (campaign caseExpired : Bool) :
    expiry true campaign caseExpired = .interrupted := by simp [expiry]

-- This invariant concerns the abstract finding state, not filesystem durability.
structure Finding where
  original : Nat
  derived : List Nat := []
  deriving DecidableEq

def append (s : Finding) (candidate : Nat) : Finding :=
  { s with derived := candidate :: s.derived }

theorem derived_preserves_original (s : Finding) (candidates : List Nat) :
    (candidates.foldl append s).original = s.original := by
  induction candidates generalizing s with
  | nil => rfl
  | cons candidate rest ih => exact ih (append s candidate)

-- Sensitivity witnesses for reversed budget precedence and replacing originals.
example : expiry false true true ≠ .caseTimeout := by decide
example : ({original := 1, derived := []} : Finding).original ≠ 2 := by decide

end BudgetModel
