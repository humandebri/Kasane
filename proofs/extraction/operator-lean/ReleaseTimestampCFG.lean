import ReleaseTimestampCorrespondence
open Aeneas Aeneas.Std
namespace ReleaseTimestampCFG
open ReleaseTimestampSequence
def executeWith (api : Calls) (t : Nat) (d : Duration) (k : Store → Outcome) : List Statement → Store → Outcome
  | [], s => k s
  | .input dst :: tail, s => executeWith api t d k tail (put s dst (.number t))
  | .asNanos dst :: tail, s => executeWith api t d k tail (put s dst (.number (api.asNanos d)))
  | .narrow dst src :: tail, s => match get s src with
    | some (.number n) => executeWith api t d k tail (put s dst (.converted (api.narrow n)))
    | _ => .invalid
  | .unwrap dst src :: tail, s => match get s src with
    | some (.converted r) => match api.unwrap r with
      | .ok n => executeWith api t d k tail (put s dst (.number n))
      | .error _ => .panic
    | _ => .invalid
  | .saturate op dst lhs rhs :: tail, s => match get s lhs, get s rhs with
    | some (.number x), some (.number y) => executeWith api t d k tail (put s dst (.number (api.saturate op x y)))
    | _, _ => .invalid
  | .returnTimestamp src :: _, s => match get s src with
    | some (.number n) => .returned n
    | _ => .invalid


theorem executeWith_append (api : Calls) (t : Nat) (d : Duration)
    (xs ys : List Statement) (s : Store) (k : Store → Outcome) :
    executeWith api t d k (xs ++ ys) s =
      executeWith api t d (executeWith api t d k ys) xs s := by
  induction xs generalizing s with
  | nil => rfl
  | cons x xs ih =>
    cases x <;> simp only [List.cons_append, executeWith, ih]

theorem executeWith_invalid (api : Calls) (t : Nat) (d : Duration)
    (xs : List Statement) (s : Store) :
    executeWith api t d (fun _ => .invalid) xs s = execute api t d xs s := by
  induction xs generalizing s with
  | nil => rfl
  | cons x xs ih =>
    cases x <;> simp only [executeWith, execute, ih] <;> rfl

structure Block where
  body : List Statement
  successor : Option Nat
abbrev Graph := Nat → Option Block

def lower : Nat → Graph → Nat → Option (List Statement)
  | 0, _, _ => none
  | fuel + 1, graph, pc => do
    let block ← graph pc
    match block.successor with
    | none => some block.body
    | some next => do
      let tail ← lower fuel graph next
      some (block.body ++ tail)

def executeCFG (api : Calls) (t : Nat) (d : Duration) : Nat → Graph → Nat → Store → Outcome
  | 0, _, _, _ => .invalid
  | fuel + 1, graph, pc, s => match graph pc with
    | none => .invalid
    | some block => executeWith api t d (fun after =>
        match block.successor with
        | none => .invalid
        | some next => executeCFG api t d fuel graph next after) block.body s

theorem lower_correct (api : Calls) (t : Nat) (d : Duration)
    (fuel : Nat) (graph : Graph) (pc : Nat) (code : List Statement)
    (valid : lower fuel graph pc = some code) (s : Store) :
    executeCFG api t d fuel graph pc s = execute api t d code s := by
  induction fuel generalizing pc code s with
  | zero => simp [lower] at valid
  | succ fuel ih =>
    cases hb : graph pc with
    | none => simp [lower, hb] at valid
    | some b =>
      cases hn : b.successor with
      | none =>
        simp [lower, hb, hn] at valid
        subst code
        simpa only [executeCFG, hb, hn] using executeWith_invalid api t d b.body s
      | some next =>
        cases hl : lower fuel graph next with
        | none => simp [lower, hb, hn, hl] at valid
        | some tail =>
          simp [lower, hb, hn, hl] at valid
          subst code
          rw [← executeWith_invalid, executeWith_append]
          simp only [executeCFG, hb, hn]
          have hcont : executeCFG api t d fuel graph next = executeWith api t d (fun _ => .invalid) tail := by
            funext after
            rw [executeWith_invalid]
            exact ih next tail hl after
          rw [hcont]
end ReleaseTimestampCFG
