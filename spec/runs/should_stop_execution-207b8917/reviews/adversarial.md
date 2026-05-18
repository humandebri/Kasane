# adversarial review: should_stop_execution

The main ambiguity is whether zero limits mean disabled limits. Existing code and
tests encode zero as disabled for both gas and instruction soft limits.
