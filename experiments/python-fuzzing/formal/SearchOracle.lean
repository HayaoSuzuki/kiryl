-- Copyright (c) 2026 kiryl contributors
import SearchModel

def corpus : String :=
  String.join (["unique", "reject", "duplicate"].flatMap fun mode =>
    [0, 1, 2, 255, 256, 257, 300].map fun n =>
      let cases := (List.range n).map fun i => (mode != "reject", if mode == "duplicate" then 0 else i)
      let s := SearchModel.run {} cases
      s!"{mode}\t{n}\t{s.entries}\t{s.seen.length}\n")

def main (args : List String) : IO UInt32 := do
  match args with
  | [] => IO.print corpus; return 0
  | ["--check", path] =>
    if (← IO.FS.readFile path) == corpus then return 0
    IO.eprintln "search corpus is stale"; return 1
  | _ => return 2
