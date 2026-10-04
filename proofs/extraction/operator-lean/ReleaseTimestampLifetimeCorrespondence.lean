import ReleaseTimestampLifetimeGenerated
namespace ReleaseTimestampLocalLifetime

theorem same_lifetime_schedule : addEvents = subEvents := rfl

theorem add_event_count : addEvents.length = 20 := rfl

theorem sub_event_count : subEvents.length = 20 := rfl

theorem add_schedule_checked (policy : Bool) : check policy addEvents initial = some finalState := by
  cases policy <;> decide

theorem sub_schedule_checked (policy : Bool) : check policy subEvents initial = some finalState := by
  cases policy <;> decide

theorem add_all_prefixes_checked (policy : Bool) (k : Nat) :
    ∃ s, check policy (addEvents.take k) initial = some s :=
  checked_prefix policy addEvents initial finalState (add_schedule_checked policy) k

theorem sub_all_prefixes_checked (policy : Bool) (k : Nat) :
    ∃ s, check policy (subEvents.take k) initial = some s :=
  checked_prefix policy subEvents initial finalState (sub_schedule_checked policy) k

theorem premature_argument_death_rejected :
    check true [.dead 1, .live 4, .copy 1 4] initial = none := by decide

theorem uninitialized_return_rejected : check true [.ret 0] initial = none := by decide

theorem double_move_rejected :
    check true [.call [8] 7, .call [8] 6] ⟨[8, 7, 6], [8]⟩ = none := by decide

theorem lifetime_restart_read_rejected :
    check true [.live 1, .ret 1] initial = none := by decide
end ReleaseTimestampLocalLifetime
