#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

cd "${WORKSPACE_DIR}"
export SLINT_LIVE_PREVIEW=1

exec cargo run -p worker-source-embedding-desktop --features live-preview "$@"
