#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
WORKERS_DIR=$(CDPATH= cd -- "${SCRIPT_DIR}/.." && pwd)
REPO_ROOT=$(CDPATH= cd -- "${WORKERS_DIR}/.." && pwd)
BACKEND_DIR="${REPO_ROOT}/backend"

source "${WORKERS_DIR}/installers/release/lib/common.sh"
source "${WORKERS_DIR}/installers/release/lib/manifest.sh"
source "${WORKERS_DIR}/installers/release/lib/catalog.sh"
source "${WORKERS_DIR}/installers/release/families/desktop.sh"
source "${WORKERS_DIR}/installers/release/families/rss.sh"
source "${WORKERS_DIR}/installers/release/families/embedding.sh"

STORAGE_ROOT="${BACKEND_DIR}/var/worker-releases"
CATALOG_PATH=""
DOWNLOAD_BASE_URL=""
RELEASE_NOTES_BASE_URL=""
SKIP_BUILD=0
DRY_RUN=0
PUBLISHED_AT=""
METADATA_PATH=""
STORAGE_ROOT_EXPLICIT=0
declare -a FAMILIES=()

usage() {
  cat <<'EOF'
Usage: release-workers.sh [options]

Options:
  --family desktop|rss|embedding Publish only the selected family. Repeatable.
  --skip-build                    Reuse existing artifacts instead of rebuilding them.
  --dry-run                       Build/stage artifacts outside backend storage and write a preview catalog.
  --storage-root PATH             Release storage root. Defaults to backend storage, or a temp dir in dry-run.
  --catalog-path PATH             Catalog JSON path. Defaults to <storage-root>/catalog.json.
  --download-base-url URL         Public backend base URL for artifact download links.
  --release-notes-base-url URL    Public release notes/download page URL.
  --published-at RFC3339          Override published_at timestamp.
  --help                          Show this help.

Defaults:
  MANIFEED_PUBLIC_BASE_URL        Public root URL. Falls back to http://localhost or
                                  http://localhost:<EDGE_HTTP_PORT>.
EOF
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --family)
        shift
        case "${1:-}" in
          desktop|rss|embedding) FAMILIES+=("$1") ;;
          *)
            printf 'Unknown family: %s\n' "${1:-}" >&2
            exit 1
            ;;
        esac
        ;;
      --skip-build)
        SKIP_BUILD=1
        ;;
      --dry-run)
        DRY_RUN=1
        ;;
      --storage-root)
        shift
        STORAGE_ROOT=${1:-}
        STORAGE_ROOT_EXPLICIT=1
        ;;
      --catalog-path)
        shift
        CATALOG_PATH=${1:-}
        ;;
      --download-base-url)
        shift
        DOWNLOAD_BASE_URL=${1:-}
        ;;
      --release-notes-base-url)
        shift
        RELEASE_NOTES_BASE_URL=${1:-}
        ;;
      --published-at)
        shift
        PUBLISHED_AT=${1:-}
        ;;
      --help|-h)
        usage
        exit 0
        ;;
      *)
        printf 'Unknown option: %s\n' "$1" >&2
        usage >&2
        exit 1
        ;;
    esac
    shift
  done
}

prepare_output_paths() {
  if [[ ${DRY_RUN} -eq 1 && ${STORAGE_ROOT_EXPLICIT} -eq 0 ]]; then
    STORAGE_ROOT=$(mktemp -d -t manifeed-worker-releases-dry-run.XXXXXX)
  fi

  if [[ -z "${CATALOG_PATH}" ]]; then
    CATALOG_PATH="${STORAGE_ROOT}/catalog.json"
  fi

  mkdir -p "${STORAGE_ROOT}"
  mkdir -p "$(dirname "${CATALOG_PATH}")"
}

main() {
  parse_args "$@"

  require_cmd python3
  require_cmd cargo
  require_cmd tar

  if [[ ${#FAMILIES[@]} -eq 0 ]]; then
    FAMILIES=(desktop rss embedding)
  fi

  if [[ -z "${PUBLISHED_AT}" ]]; then
    PUBLISHED_AT=$(resolve_published_at)
  fi

  prepare_output_paths

  local public_base_url
  public_base_url=$(default_public_base_url)
  if [[ -z "${DOWNLOAD_BASE_URL}" ]]; then
    DOWNLOAD_BASE_URL="${public_base_url}/workers/api/releases/download"
  fi
  if [[ -z "${RELEASE_NOTES_BASE_URL}" ]]; then
    RELEASE_NOTES_BASE_URL="${public_base_url}/workers"
  fi
  DOWNLOAD_BASE_URL=$(normalize_url_base "${DOWNLOAD_BASE_URL}")
  RELEASE_NOTES_BASE_URL=$(normalize_url_base "${RELEASE_NOTES_BASE_URL}")

  METADATA_PATH=$(mktemp)
  trap 'rm -f "${METADATA_PATH:-}"' EXIT

  local platform arch
  local desktop_version rss_version rss_worker_version embedding_version embedding_worker_version
  platform=$(current_release_platform)
  arch=$(current_release_arch)

  desktop_version=$(resolve_artifact_version "${WORKERS_DIR}/worker-desktop/Cargo.toml" "${platform}" "${arch}")
  rss_version=$(resolve_artifact_version "${WORKERS_DIR}/worker-rss/Cargo.toml" "${platform}" "${arch}")
  rss_worker_version=$(resolve_worker_version_metadata "${WORKERS_DIR}/worker-rss/Cargo.toml")
  if [[ -z "${rss_worker_version}" ]]; then
    rss_worker_version=$(resolve_package_version "${WORKERS_DIR}/worker-rss/Cargo.toml")
  fi
  embedding_version=$(resolve_artifact_version "${WORKERS_DIR}/worker-source-embedding/Cargo.toml" "${platform}" "${arch}")
  embedding_worker_version=$(resolve_worker_version_metadata "${WORKERS_DIR}/worker-source-embedding/Cargo.toml")
  if [[ -z "${embedding_worker_version}" ]]; then
    embedding_worker_version=$(resolve_package_version "${WORKERS_DIR}/worker-source-embedding/Cargo.toml")
  fi

  for family in "${FAMILIES[@]}"; do
    case "${family}" in
      desktop)
        case "${platform}" in
          linux) publish_linux_desktop "${desktop_version}" ;;
          macos) publish_macos_desktop "${desktop_version}" ;;
        esac
        ;;
      rss)
        publish_rss_family "${platform}" "${arch}" "${rss_version}" "${rss_worker_version}"
        ;;
      embedding)
        case "${platform}" in
          linux) publish_embedding_family_linux "${arch}" "${embedding_version}" "${embedding_worker_version}" ;;
          macos) publish_embedding_family_macos "${arch}" "${embedding_version}" "${embedding_worker_version}" ;;
        esac
        ;;
    esac
  done

  if [[ ! -s "${METADATA_PATH}" ]]; then
    printf 'No release artifacts were produced.\n' >&2
    exit 1
  fi

  update_catalog \
    "${METADATA_PATH}" \
    "${CATALOG_PATH}" \
    "${DOWNLOAD_BASE_URL}" \
    "${RELEASE_NOTES_BASE_URL}" \
    "${PUBLISHED_AT}"

  if [[ ${DRY_RUN} -eq 1 ]]; then
    printf 'Dry run artifacts staged under %s\n' "${STORAGE_ROOT}"
  else
    printf 'Artifacts stored under %s\n' "${STORAGE_ROOT}"
  fi
  printf 'Catalog updated: %s\n' "${CATALOG_PATH}"
}

main "$@"
