import ReleaseTimestampLifetimeCorrespondence
namespace ReleaseTimestampLifetimeSoundness
open ReleaseTimestampLocalLifetime

def readLocals : Event → List Nat
  | .live _ | .dead _ => []
  | .copy src _ | .borrow src _ | .move src _ => [src]
  | .call args _ => args
  | .ret src => [src]

def writeLocals : Event → List Nat
  | .copy _ dst | .borrow _ dst | .move _ dst | .call _ dst => [dst]
  | _ => []

def ReadsInitialized (event : Event) (s : State) : Prop :=
  ∀ i ∈ readLocals event, s.alive.contains i = true ∧ s.initialized.contains i = true

def WritesLive (event : Event) (s : State) : Prop :=
  ∀ i ∈ writeLocals event, s.alive.contains i = true

theorem step_sound (policy : Bool) (event : Event) (before after : State)
    (accepted : step policy event before = some after) :
    ReadsInitialized event before ∧ WritesLive event before := by
  cases event <;> simp only [step] at accepted
  all_goals try (solve | simp [ReadsInitialized, WritesLive, readLocals, writeLocals])
  all_goals split at accepted <;>
    simp_all [ReadsInitialized, WritesLive, readLocals, writeLocals, ready, List.all_eq_true]

def SafeTrace (policy : Bool) : List Event → State → Prop
  | [], _ => True
  | event :: tail, before => ∃ after,
      step policy event before = some after ∧ ReadsInitialized event before ∧
      WritesLive event before ∧ SafeTrace policy tail after

theorem check_sound (policy : Bool) (events : List Event) (before after : State)
    (accepted : check policy events before = some after) : SafeTrace policy events before := by
  induction events generalizing before with
  | nil => trivial
  | cons event tail ih =>
    cases hs : step policy event before with
    | none => simp [check, hs] at accepted
    | some next =>
      have rest : check policy tail next = some after := by simpa [check, hs] using accepted
      have sound := step_sound policy event before next hs
      exact ⟨next, hs, sound.1, sound.2, ih next rest⟩

theorem add_trace_safe (policy : Bool) : SafeTrace policy addEvents initial :=
  check_sound policy addEvents initial finalState (add_schedule_checked policy)

theorem sub_trace_safe (policy : Bool) : SafeTrace policy subEvents initial :=
  check_sound policy subEvents initial finalState (sub_schedule_checked policy)

theorem add_prefix_safe (policy : Bool) (k : Nat) : SafeTrace policy (addEvents.take k) initial := by
  obtain ⟨after, checked⟩ := add_all_prefixes_checked policy k
  exact check_sound policy _ initial after checked

theorem sub_prefix_safe (policy : Bool) (k : Nat) : SafeTrace policy (subEvents.take k) initial := by
  obtain ⟨after, checked⟩ := sub_all_prefixes_checked policy k
  exact check_sound policy _ initial after checked
end ReleaseTimestampLifetimeSoundness
