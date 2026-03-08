# Indexer Runbook v2.1 (TS + Postgres + local zstd archive)

Japanese version: [./indexer-runbook.ja.md](./indexer-runbook.ja.md)

## Purpose
Operational runbook for indexer setup, recovery, migration, archive, pruning, and production checks.

## Covered Operations
- invariants and component map
- local `icp-cli` recovery (managed network, port 8000)
- startup/shutdown and environment configuration
- log interpretation (JSON lines)
- Postgres migrations
- zstd archive format, atomicity, and startup GC
- metrics/retention operations
- common incidents and recovery patterns
- staged pruning enablement and prune status monitoring
- local integrated smoke and failure injection
- 24h capacity measurement and deploy checklist
- rollback and prune-linked operation rules

## Usage
This English file is the canonical high-level guide.
For full step-by-step command procedures and operational thresholds, use the Japanese version.
