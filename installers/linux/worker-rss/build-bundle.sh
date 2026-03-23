#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
WORKSPACE_ROOT=$(CDPATH= cd -- "${SCRIPT_DIR}/../../.." && pwd)
DIST_DIR="${WORKSPACE_ROOT}/dist/linux/worker-rss"
COMMON_ICON_PATH="${SCRIPT_DIR}/../manifeed-workers.svg"

mkdir -p "$DIST_DIR"

cargo build --release -p worker-rss -p worker-source-embedding-desktop --manifest-path "${WORKSPACE_ROOT}/Cargo.toml"

install -m 0755 "${WORKSPACE_ROOT}/target/release/worker-rss" "${DIST_DIR}/worker-rss"
install -m 0755 "${WORKSPACE_ROOT}/target/release/manifeed-workers" "${DIST_DIR}/manifeed-workers"
install -m 0755 "${SCRIPT_DIR}/install.sh" "${DIST_DIR}/install.sh"
install -m 0644 "${SCRIPT_DIR}/README.md" "${DIST_DIR}/README.md"
install -m 0644 "${COMMON_ICON_PATH}" "${DIST_DIR}/manifeed-workers.svg"

printf 'Bundle ready in %s\n' "$DIST_DIR"
