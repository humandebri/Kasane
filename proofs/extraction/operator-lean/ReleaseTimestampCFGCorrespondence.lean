import ReleaseTimestampCFGGenerated
open Aeneas Aeneas.Std
namespace ReleaseTimestampCFG
open ReleaseTimestampSequence

theorem add_lower : lower 5 releaseAddCFG 0 = some releaseAdd := by rfl

theorem sub_lower : lower 5 releaseSubCFG 0 = some releaseSub := by rfl

theorem add_graph_equiv (api : Calls) (t : Nat) (d : Duration) (s : Store) :
    executeCFG api t d 5 releaseAddCFG 0 s = execute api t d releaseAdd s :=
  lower_correct api t d 5 releaseAddCFG 0 releaseAdd add_lower s

theorem sub_graph_equiv (api : Calls) (t : Nat) (d : Duration) (s : Store) :
    executeCFG api t d 5 releaseSubCFG 0 s = execute api t d releaseSub s :=
  lower_correct api t d 5 releaseSubCFG 0 releaseSub sub_lower s

theorem add_graph_conditional (api : Calls) (contracts : CallContracts api)
    (t : U64) (d : Duration) :
    executeCFG api t.val d 5 releaseAddCFG 0 [] = expected .add t.val d := by
  rw [add_graph_equiv]
  exact release_add_conditional api contracts t d

theorem sub_graph_conditional (api : Calls) (contracts : CallContracts api)
    (t : U64) (d : Duration) :
    executeCFG api t.val d 5 releaseSubCFG 0 [] = expected .sub t.val d := by
  rw [sub_graph_equiv]
  exact release_sub_conditional api contracts t d

theorem add_insufficient_fuel : lower 4 releaseAddCFG 0 = none := by rfl

theorem sub_insufficient_fuel : lower 4 releaseSubCFG 0 = none := by rfl
end ReleaseTimestampCFG
