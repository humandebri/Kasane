# Verify Runbook

Japanese version: [./verify_runbook.ja.md](./verify_runbook.ja.md)

## Purpose
Operational runbook for verify pipeline readiness, key rotation, and incident triage.

## Covered Operations
1. pre-release checks (`solc`)
2. metrics operation (restart fluctuation handling)
3. key rotation (`kid`) lifecycle: pre-check -> switch -> old-key cleanup
4. audit-hash salt rotation
5. JTI replay-table operation
6. duplicate-judgment rules
7. first-step failure triage

## Usage
Use this file as the operational index.
For concrete command examples and parameter details, follow the Japanese version.
