# implementation review

No blocking issue.

- ACCEPT accepts only when new pending/current/location evidence is present and old tx removal is false.
- REPLACE accepts only when the same write evidence is present and old tx removal is true.
- Unknown decision codes return false.

Residual boundary: adapter-provided evidence booleans must be derived from real pending and queue state.
