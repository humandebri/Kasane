# adversarial review: classify_nonce

The scenarios distinguish all returned `NonceDecision` variants.

Main risk: this remains implementation-derived. If business policy requires a replacement bump threshold greater than `old + 1` or a percentage bump, the current implementation and draft would both encode the wrong intended behavior.

Human review should confirm that strict `>` is the intended replacement rule.
