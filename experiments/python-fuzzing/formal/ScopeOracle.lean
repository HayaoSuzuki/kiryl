import ScopeModel

def corpus : String :=
  String.join ((List.range 9).map fun n =>
    let s := ScopeModel.replay n
    s!"{n}\t{s.starts}\t{s.finishes}\n")

def main (args : List String) : IO UInt32 := do
  match args with
  | ["--check", path] =>
    let actual ← IO.FS.readFile path
    if actual == corpus then return 0
    IO.eprintln "scope corpus is stale"
    return 1
  | [] => IO.print corpus; return 0
  | _ => IO.eprintln "usage: scope-oracle [--check PATH]"; return 2
