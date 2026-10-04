import Lean

/- Projection positions for one symbolic value under two region views.
   `used` describes positions actually carrying borrows in the chosen family.
   A Rust/LLBC embedding must account for GAT arguments, trait/type arguments,
   captured borrows and implementation selection; that embedding is unproved.
   These are conditional model obligations, not an Aeneas comparison patch.
-/
namespace GatBorrowFootprint

abbrev Selection (Position : Type) := Position → Prop

def intersects {Position : Type} (used left right : Selection Position) : Prop :=
  ∃ i, used i ∧ left i ∧ right i

def contains {Position : Type} (used left right : Selection Position) : Prop :=
  ∀ i, used i → right i → left i

/-- Comparing every potential position overapproximates intersection. -/
def argumentIntersects {Position : Type} (left right : Selection Position) : Prop :=
  ∃ i, left i ∧ right i

/-- Coverage at every potential position implies coverage at used positions. -/
def argumentContains {Position : Type} (left right : Selection Position) : Prop :=
  ∀ i, right i → left i

theorem intersection_overapproximation {Position : Type}
    (used left right : Selection Position) :
    intersects used left right → argumentIntersects left right := by
  rintro ⟨i, _, hl, hr⟩
  exact ⟨i, hl, hr⟩

theorem argument_disjointness_is_safe {Position : Type}
    (used left right : Selection Position) :
    ¬ argumentIntersects left right → ¬ intersects used left right := by
  intro hno hactual
  exact hno (intersection_overapproximation used left right hactual)

theorem argument_containment_is_safe {Position : Type}
    (used left right : Selection Position) :
    argumentContains left right → contains used left right := by
  intro h i _ hr
  exact h i hr

theorem complete_support_intersection {Position : Type}
    (used left right : Selection Position) (hcomplete : ∀ i, used i) :
    intersects used left right ↔ argumentIntersects left right := by
  constructor
  · exact intersection_overapproximation used left right
  · rintro ⟨i, hl, hr⟩
    exact ⟨i, hcomplete i, hl, hr⟩

theorem complete_support_containment {Position : Type}
    (used left right : Selection Position) (hcomplete : ∀ i, used i) :
    contains used left right ↔ argumentContains left right := by
  constructor
  · intro h i hr
    exact h i (hcomplete i) hr
  · exact argument_containment_is_safe used left right

theorem unused_support_has_no_intersection {Position : Type}
    (used left right : Selection Position) (hunused : ∀ i, ¬ used i) :
    ¬ intersects used left right := by
  rintro ⟨i, hu, _, _⟩
  exact hunused i hu

theorem unused_support_is_contained {Position : Type}
    (used left right : Selection Position) (hunused : ∀ i, ¬ used i) :
    contains used left right := by
  intro i hu _
  exact False.elim (hunused i hu)

/-- A regionless opaque model's false intersection misses a possible borrow. -/
theorem regionless_intersection_counterexample :
    intersects (fun _ : Unit => True) (fun _ => True) (fun _ => True) ∧ ¬ False := by
  exact ⟨⟨(), True.intro, True.intro, True.intro⟩, id⟩

/-- Its true containment can accept a missing borrow projection. -/
theorem regionless_containment_counterexample :
    True ∧ ¬ contains (fun _ : Unit => True) (fun _ => False) (fun _ => True) := by
  constructor
  · trivial
  · intro h
    exact h () True.intro True.intro

/-- An unused lifetime can make argument intersection a false positive. -/
theorem unused_lifetime_intersection_counterexample :
    argumentIntersects (fun _ : Unit => True) (fun _ => True) ∧
      ¬ intersects (fun _ : Unit => False) (fun _ => True) (fun _ => True) := by
  constructor
  · exact ⟨(), True.intro, True.intro⟩
  · rintro ⟨_, hu, _, _⟩
    exact hu

/-- Conservative argument containment can reject an actually empty footprint. -/
theorem unused_lifetime_containment_counterexample :
    contains (fun _ : Unit => False) (fun _ => False) (fun _ => True) ∧
      ¬ argumentContains (fun _ : Unit => False) (fun _ => True) := by
  constructor
  · intro _ hu _
    exact hu
  · intro h
    exact h () True.intro

end GatBorrowFootprint
