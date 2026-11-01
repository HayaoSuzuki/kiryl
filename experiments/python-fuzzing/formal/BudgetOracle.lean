-- Copyright (c) 2026 kiryl contributors
import BudgetModel

open BudgetModel

def corpus : String :=
  String.join (([false, true].flatMap fun i => [false, true].flatMap fun g =>
    [false, true].map fun c =>
      let result := match expiry i g c with
        | .running => "running"
        | .interrupted => "interrupted"
        | .campaign => "campaign_cancelled"
        | .caseTimeout => "case_timeout"
      s!"{i}\t{g}\t{c}\t{result}\n"))

def main (args : List String) : IO UInt32 := do
  match args with
  | [] => IO.print corpus; return 0
  | ["--check", path] =>
    if (← IO.FS.readFile path) == corpus then return 0
    IO.eprintln "budget corpus is stale"; return 1
  | _ => return 2
