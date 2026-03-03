# Schema Migration Runbook

Japanese version: [./schema-migration-runbook.ja.md](./schema-migration-runbook.ja.md)

## Purpose
Safe operational procedure for schema migration with pre-check, execution, and incident recovery.

## Required Preparation
- verify environment/target and migration version
- ensure backup/restore path is available
- verify permissions and maintenance window

## Execution
1. run pre-checks
2. apply migration in planned order
3. run post-migration verification

## Incident Recovery
If errors occur, stop rollout and recover using backup/rollback steps in the Japanese runbook.

## Notes
The Japanese version contains detailed command examples and operational cautions.
