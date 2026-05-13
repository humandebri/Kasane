#!/usr/bin/env bash
# where: smoke runner
# what: run read-only smoke checks in order
# why: unify the compatibility verification sequence
set -euo pipefail

npm run smoke:read
