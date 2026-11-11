import SchedulerModel
open SchedulerModel

def options : List (Nat × Bool) :=
  [0, 1, 7].flatMap fun charge => [false, true].map fun stop => (charge, stop)

def row (xs : List (Nat × Bool)) : String :=
  let charges := String.intercalate "," (xs.map fun x => toString x.1)
  let stops := String.intercalate "," (xs.map fun x => if x.2 then "1" else "0")
  let expected := match choose xs with | none => "none" | some i => toString i
  s!"{charges}\t{stops}\t{expected}\n"

def corpus : String :=
  "# scheduler-v1\n" ++ row [] ++ String.join (options.flatMap fun a =>
    options.flatMap fun b => options.map fun c => row [a,b,c])

def main (args : List String) : IO UInt32 := do
  match args with
  | [] => IO.print corpus; return 0
  | ["--check", path] =>
    if (← IO.FS.readFile path) == corpus then return 0
    IO.eprintln "scheduler corpus is stale"; return 1
  | _ => return 2
