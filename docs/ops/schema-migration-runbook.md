# Schema Migration Runbook

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
If errors occur, stop rollout and recover using the prepared backup/rollback steps.
