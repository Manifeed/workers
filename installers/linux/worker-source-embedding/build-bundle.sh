#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
WORKSPACE_ROOT=$(CDPATH= cd -- "${SCRIPT_DIR}/../../.." && pwd)
DIST_DIR="${WORKSPACE_ROOT}/dist/linux/worker-source-embedding"

mkdir -p "$DIST_DIR"

cargo build --release -p worker-source-embedding -p worker-source-embedding-desktop --manifest-path "${WORKSPACE_ROOT}/Cargo.toml"

install -m 0755 "${WORKSPACE_ROOT}/target/release/worker-source-embedding" "${DIST_DIR}/worker-source-embedding"
install -m 0755 "${WORKSPACE_ROOT}/target/release/worker-source-embedding-desktop" "${DIST_DIR}/worker-source-embedding-desktop"
install -m 0755 "${SCRIPT_DIR}/install.sh" "${DIST_DIR}/install.sh"
install -m 0644 "${SCRIPT_DIR}/README.md" "${DIST_DIR}/README.md"
install -m 0644 "${SCRIPT_DIR}/manifeed-worker-source-embedding.svg" "${DIST_DIR}/manifeed-worker-source-embedding.svg"

printf 'Bundle ready in %s\n' "$DIST_DIR"
