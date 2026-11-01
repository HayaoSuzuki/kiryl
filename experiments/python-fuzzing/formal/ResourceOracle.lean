-- Copyright (c) 2026 kiryl contributors
import ResourceModel

def corpus : String :=
  String.join ((List.range 9).map fun n =>
    let s := ResourceModel.requests n
    s!"{n}\t{s.consumed}\t{s.attempts}\n")

def main (args : List String) : IO UInt32 := do
  match args with
  | [] => IO.print corpus; return 0
  | ["--check", path] =>
    if (← IO.FS.readFile path) == corpus then return 0
    IO.eprintln "resource corpus is stale"; return 1
  | _ => return 2
