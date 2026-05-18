# implementation review: should_stop_execution

The draft matches the implementation and existing Verus annotation. Stop is the
disjunction of gas-limit exhaustion and instruction soft-limit exhaustion.
