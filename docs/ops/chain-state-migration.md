# ChainState 72->88 Migration Runbook

Japanese version: [./chain-state-migration.ja.md](./chain-state-migration.ja.md)

## Purpose
Operational procedure for migrating ChainState schema/version from 72 to 88 safely.

## Scope Decision
Use this runbook when target canister state is on the 72-line and must be upgraded to 88-line data layout.

## Required Preparation
- confirm target canister/network and current version
- take backup/snapshot before migration
- confirm operator identity/permissions
- stop write-heavy jobs if required

## Execution Steps
1. run pre-checks
2. execute migration command(s)
3. verify post-migration state and method behavior

## Validation
- schema/version moved to expected target
- key query/update APIs respond without regression
- logs/metrics show no migration errors

## Rollback
If validation fails, stop traffic and restore from backup according to the rollback section in the Japanese runbook.

## Notes
The Japanese version remains the full operational source for detailed command-level procedures.
