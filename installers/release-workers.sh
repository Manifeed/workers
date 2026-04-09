#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
WORKERS_DIR=$(CDPATH= cd -- "${SCRIPT_DIR}/.." && pwd)
REPO_ROOT=$(CDPATH= cd -- "${WORKERS_DIR}/.." && pwd)
BACKEND_DIR="${REPO_ROOT}/backend"
STORAGE_ROOT="${BACKEND_DIR}/var/worker-releases"
CATALOG_PATH="${STORAGE_ROOT}/catalog.json"
DOWNLOAD_BASE_URL="http://localhost:8000/workers/releases/download"
RELEASE_NOTES_BASE_URL="http://localhost:3000/app/workers"
SKIP_BUILD=0
PUBLISHED_AT=""
METADATA_PATH=""
declare -a FAMILIES=()

usage() {
  cat <<'EOF'
Usage: release-workers.sh [options]

Options:
  --family desktop|rss|embedding Publish only the selected family. Repeatable.
  --skip-build                    Reuse existing artifacts instead of rebuilding them.
  --storage-root PATH             Backend storage root for artifacts and catalog.
  --catalog-path PATH             Catalog JSON path. Defaults to <storage-root>/catalog.json.
  --download-base-url URL         Public backend base URL for artifact download links.
  --release-notes-base-url URL    Public release notes/download page URL.
  --published-at RFC3339          Override published_at timestamp.
  --help                          Show this help.
EOF
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'Missing required command: %s\n' "$1" >&2
    exit 1
  }
}

normalize_url_base() {
  printf '%s' "${1%/}"
}

current_os() {
  uname -s | tr '[:upper:]' '[:lower:]'
}

current_release_platform() {
  case "$(current_os)" in
    linux) printf 'linux\n' ;;
    darwin) printf 'macos\n' ;;
    *)
      printf 'Unsupported host OS: %s\n' "$(uname -s)" >&2
      exit 1
      ;;
  esac
}

current_release_arch() {
  case "$(uname -m)" in
    x86_64|amd64) printf 'x86_64\n' ;;
    aarch64|arm64) printf 'aarch64\n' ;;
    *)
      printf 'Unsupported host architecture: %s\n' "$(uname -m)" >&2
      exit 1
      ;;
  esac
}

current_deb_arch() {
  case "$(current_release_arch)" in
    x86_64) printf 'amd64\n' ;;
    aarch64) printf 'arm64\n' ;;
  esac
}

resolve_published_at() {
  python3 - <<'PY'
from datetime import datetime, timezone
print(datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"))
PY
}

resolve_package_version() {
  local manifest_path=$1
  python3 - "${manifest_path}" <<'PY'
import tomllib
import sys
from pathlib import Path

path = Path(sys.argv[1])
data = tomllib.loads(path.read_text(encoding="utf-8"))
print(data["package"]["version"])
PY
}

resolve_artifact_version() {
  local manifest_path=$1
  local platform=$2
  local arch=$3
  python3 - "${manifest_path}" "${platform}" "${arch}" <<'PY'
import tomllib
import sys
from pathlib import Path

path = Path(sys.argv[1])
platform = sys.argv[2].strip().lower().replace("-", "_")
arch = sys.argv[3].strip().lower().replace("-", "_")
data = tomllib.loads(path.read_text(encoding="utf-8"))
package = data["package"]
release = package.get("metadata", {}).get("manifeed", {}).get("release", {})
override_key = f"artifact_version_{platform}_{arch}"
override_value = str(release.get(override_key, "")).strip()
print(override_value or package["version"])
PY
}

resolve_worker_version_metadata() {
  local manifest_path=$1
  python3 - "${manifest_path}" <<'PY'
import tomllib
import sys
from pathlib import Path

path = Path(sys.argv[1])
data = tomllib.loads(path.read_text(encoding="utf-8"))
release = data.get("package", {}).get("metadata", {}).get("manifeed", {}).get("release", {})
worker_version = str(release.get("worker_version", "")).strip()
print(worker_version)
PY
}

copy_file() {
  local source=$1
  local destination=$2
  install -d "$(dirname "${destination}")"
  cp -f "${source}" "${destination}"
}

latest_matching_file() {
  local directory=$1
  local pattern=$2
  find "${directory}" -maxdepth 1 -type f -name "${pattern}" -printf '%T@ %p\n' 2>/dev/null \
    | sort -nr \
    | head -n 1 \
    | cut -d' ' -f2-
}

write_bundle_manifest() {
  local destination=$1
  local product=$2
  local version=$3
  local worker_version=$4
  local runtime_bundle=$5

  if [[ -n "${runtime_bundle}" ]]; then
    cat > "${destination}" <<EOF
{
  "product": "${product}",
  "version": "${version}",
  "worker_version": "${worker_version}",
  "runtime_bundle": "${runtime_bundle}"
}
EOF
  else
    cat > "${destination}" <<EOF
{
  "product": "${product}",
  "version": "${version}",
  "worker_version": "${worker_version}"
}
EOF
  fi
}

pack_bundle_directory() {
  local source_dir=$1
  local output_path=$2
  install -d "$(dirname "${output_path}")"
  tar -czf "${output_path}" -C "$(dirname "${source_dir}")" "$(basename "${source_dir}")"
}

build_rss_bundle() {
  local platform=$1
  local arch=$2
  local version=$3
  local worker_version=$4
  local output_path=$5
  local work_dir bundle_dir
  work_dir=$(mktemp -d)
  bundle_dir="${work_dir}/rss_worker_bundle-${version}-${arch}"
  trap 'rm -rf "${work_dir}"' RETURN

  install -d "${bundle_dir}/bin"
  install -m 0755 "${WORKERS_DIR}/target/release/worker-rss" \
    "${bundle_dir}/bin/worker-rss"
  write_bundle_manifest \
    "${bundle_dir}/manifest.json" \
    "rss_worker_bundle" \
    "${version}" \
    "${worker_version}" \
    ""
  pack_bundle_directory "${bundle_dir}" "${output_path}"

  rm -rf "${work_dir}"
  trap - RETURN
}

build_embedding_bundle_linux() {
  local arch=$1
  local version=$2
  local worker_version=$3
  local runtime_bundle=$4
  local output_path=$5
  local work_dir bundle_dir deb_arch runtime_mode
  work_dir=$(mktemp -d)
  bundle_dir="${work_dir}/embedding_worker_bundle-${version}-${arch}-${runtime_bundle}"
  trap 'rm -rf "${work_dir}"' RETURN

  install -d "${bundle_dir}/bin" "${bundle_dir}/runtime"
  install -m 0755 "${WORKERS_DIR}/target/release/worker-source-embedding" \
    "${bundle_dir}/bin/worker-source-embedding"

  case "${arch}" in
    x86_64) deb_arch="amd64" ;;
    aarch64) deb_arch="arm64" ;;
    *)
      printf 'Unsupported Linux bundle architecture: %s\n' "${arch}" >&2
      exit 1
      ;;
  esac
  case "${runtime_bundle}" in
    none) runtime_mode="cpu" ;;
    cuda12) runtime_mode="cuda12" ;;
    *)
      printf 'Unsupported Linux runtime bundle: %s\n' "${runtime_bundle}" >&2
      exit 1
      ;;
  esac

  bash "${WORKERS_DIR}/installers/debian/scripts/stage-onnx-runtime.sh" \
    "${runtime_mode}" \
    "${deb_arch}" \
    "${bundle_dir}/runtime"

  write_bundle_manifest \
    "${bundle_dir}/manifest.json" \
    "embedding_worker_bundle" \
    "${version}" \
    "${worker_version}" \
    "${runtime_bundle}"
  pack_bundle_directory "${bundle_dir}" "${output_path}"

  rm -rf "${work_dir}"
  trap - RETURN
}

build_embedding_bundle_macos() {
  local arch=$1
  local version=$2
  local worker_version=$3
  local runtime_bundle=$4
  local runtime_dir=$5
  local output_path=$6
  local work_dir bundle_dir
  work_dir=$(mktemp -d)
  bundle_dir="${work_dir}/embedding_worker_bundle-${version}-${arch}-${runtime_bundle}"
  trap 'rm -rf "${work_dir}"' RETURN

  install -d "${bundle_dir}/bin" "${bundle_dir}/runtime"
  install -m 0755 "${WORKERS_DIR}/target/release/worker-source-embedding" \
    "${bundle_dir}/bin/worker-source-embedding"
  cp -a "${runtime_dir}/." "${bundle_dir}/runtime/"

  write_bundle_manifest \
    "${bundle_dir}/manifest.json" \
    "embedding_worker_bundle" \
    "${version}" \
    "${worker_version}" \
    "${runtime_bundle}"
  pack_bundle_directory "${bundle_dir}" "${output_path}"

  rm -rf "${work_dir}"
  trap - RETURN
}

append_catalog_metadata() {
  local source_path=$1
  local family=$2
  local product=$3
  local platform=$4
  local arch=$5
  local version=$6
  local worker_version=$7
  local runtime_bundle=$8
  local artifact_kind=$9
  local download_auth=${10}
  local storage_relative_path=${11}
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "${source_path}" \
    "${family}" \
    "${product}" \
    "${platform}" \
    "${arch}" \
    "${version}" \
    "${worker_version}" \
    "${runtime_bundle}" \
    "${artifact_kind}" \
    "${download_auth}" \
    "${storage_relative_path}" >> "${METADATA_PATH}"
}

publish_linux_desktop() {
  local desktop_version=$1
  local dist_dir="${WORKERS_DIR}/dist/debian"
  local source basename arch storage_relative_path destination

  if [[ ${SKIP_BUILD} -eq 0 ]]; then
    MANIFEED_DESKTOP_APP_VERSION="${desktop_version}" \
    MANIFEED_DESKTOP_DEB_VERSION="${desktop_version}-1" \
      bash "${WORKERS_DIR}/installers/debian/build-debs.sh"
  fi

  source=$(latest_matching_file "${dist_dir}" "manifeed-workers-desktop_*_*.deb")
  if [[ -z "${source}" ]]; then
    printf 'No Linux desktop .deb found under %s\n' "${dist_dir}" >&2
    exit 1
  fi

  basename=$(basename "${source}")
  case "${basename##*_}" in
    amd64.deb) arch="x86_64" ;;
    arm64.deb) arch="aarch64" ;;
    *)
      printf 'Unsupported Debian desktop package name: %s\n' "${basename}" >&2
      exit 1
      ;;
  esac

  storage_relative_path="desktop/${basename}"
  destination="${STORAGE_ROOT}/${storage_relative_path}"
  copy_file "${source}" "${destination}"
  append_catalog_metadata \
    "${destination}" \
    "desktop" \
    "manifeed-workers-desktop" \
    "linux" \
    "${arch}" \
    "${desktop_version}" \
    "" \
    "" \
    "deb_package" \
    "public" \
    "${storage_relative_path}"
}

publish_macos_desktop() {
  local desktop_version=$1
  local source="${WORKERS_DIR}/dist/macos/Manifeed Workers.dmg"
  local artifact_name="Manifeed-Workers-${desktop_version}.dmg"
  local storage_relative_path="desktop/${artifact_name}"
  local destination="${STORAGE_ROOT}/${storage_relative_path}"

  if [[ ${SKIP_BUILD} -eq 0 ]]; then
    MANIFEED_DESKTOP_APP_VERSION="${desktop_version}" \
      bash "${WORKERS_DIR}/installers/macos/build-dmg.sh"
  fi

  if [[ ! -f "${source}" ]]; then
    printf 'No macOS desktop dmg found at %s\n' "${source}" >&2
    exit 1
  fi

  copy_file "${source}" "${destination}"
  append_catalog_metadata \
    "${destination}" \
    "desktop" \
    "workers_desktop_app" \
    "macos" \
    "aarch64" \
    "${desktop_version}" \
    "" \
    "" \
    "desktop_app" \
    "public" \
    "${storage_relative_path}"
}

publish_rss_family() {
  local platform=$1
  local arch=$2
  local version=$3
  local worker_version=$4
  local output_dir="${WORKERS_DIR}/dist/bundles/${platform}"
  local artifact_name="rss_worker_bundle-${version}-${platform}-${arch}.tar.gz"
  local source="${output_dir}/${artifact_name}"
  local storage_relative_path="rss/${artifact_name}"
  local destination="${STORAGE_ROOT}/${storage_relative_path}"

  if [[ ${SKIP_BUILD} -eq 0 ]]; then
    cargo build --release -p worker-rss --manifest-path "${WORKERS_DIR}/Cargo.toml"
    build_rss_bundle "${platform}" "${arch}" "${version}" "${worker_version}" "${source}"
  fi

  if [[ ! -f "${source}" ]]; then
    printf 'RSS bundle not found at %s\n' "${source}" >&2
    exit 1
  fi

  copy_file "${source}" "${destination}"
  append_catalog_metadata \
    "${destination}" \
    "rss" \
    "rss_worker_bundle" \
    "${platform}" \
    "${arch}" \
    "${version}" \
    "${worker_version}" \
    "" \
    "worker_bundle" \
    "worker_bearer" \
    "${storage_relative_path}"
}

publish_embedding_family_linux() {
  local arch=$1
  local version=$2
  local worker_version=$3
  local output_dir="${WORKERS_DIR}/dist/bundles/linux"
  local runtime_bundle artifact_name source storage_relative_path destination

  if [[ ${SKIP_BUILD} -eq 0 ]]; then
    cargo build --release -p worker-source-embedding --manifest-path "${WORKERS_DIR}/Cargo.toml"
  fi

  for runtime_bundle in none $( [[ "${arch}" == "x86_64" ]] && printf 'cuda12' ); do
    artifact_name="embedding_worker_bundle-${version}-linux-${arch}-${runtime_bundle}.tar.gz"
    source="${output_dir}/${artifact_name}"
    if [[ ${SKIP_BUILD} -eq 0 ]]; then
      build_embedding_bundle_linux "${arch}" "${version}" "${worker_version}" "${runtime_bundle}" "${source}"
    fi

    if [[ ! -f "${source}" ]]; then
      printf 'Embedding bundle not found at %s\n' "${source}" >&2
      exit 1
    fi

    storage_relative_path="embedding/${artifact_name}"
    destination="${STORAGE_ROOT}/${storage_relative_path}"
    copy_file "${source}" "${destination}"
    append_catalog_metadata \
      "${destination}" \
      "embedding" \
      "embedding_worker_bundle" \
      "linux" \
      "${arch}" \
      "${version}" \
      "${worker_version}" \
      "${runtime_bundle}" \
      "worker_bundle" \
      "worker_bearer" \
      "${storage_relative_path}"
  done
}

publish_embedding_family_macos() {
  local arch=$1
  local version=$2
  local worker_version=$3
  local output_dir="${WORKERS_DIR}/dist/bundles/macos"
  local built_any=0
  local runtime_bundle runtime_dir artifact_name source storage_relative_path destination

  if [[ ${SKIP_BUILD} -eq 0 ]]; then
    cargo build --release -p worker-source-embedding --manifest-path "${WORKERS_DIR}/Cargo.toml"
  fi

  for runtime_bundle in none coreml; do
    case "${runtime_bundle}" in
      none) runtime_dir="${MANIFEED_MACOS_CPU_RUNTIME_DIR:-${WORKERS_DIR}/dist/macos/cpu-runtime}" ;;
      coreml) runtime_dir="${MANIFEED_MACOS_COREML_RUNTIME_DIR:-${WORKERS_DIR}/dist/macos/coreml-runtime}" ;;
    esac
    if [[ ! -d "${runtime_dir}" ]]; then
      continue
    fi

    artifact_name="embedding_worker_bundle-${version}-macos-${arch}-${runtime_bundle}.tar.gz"
    source="${output_dir}/${artifact_name}"
    if [[ ${SKIP_BUILD} -eq 0 ]]; then
      build_embedding_bundle_macos \
        "${arch}" \
        "${version}" \
        "${worker_version}" \
        "${runtime_bundle}" \
        "${runtime_dir}" \
        "${source}"
    fi

    if [[ ! -f "${source}" ]]; then
      printf 'Embedding bundle not found at %s\n' "${source}" >&2
      exit 1
    fi

    built_any=1
    storage_relative_path="embedding/${artifact_name}"
    destination="${STORAGE_ROOT}/${storage_relative_path}"
    copy_file "${source}" "${destination}"
    append_catalog_metadata \
      "${destination}" \
      "embedding" \
      "embedding_worker_bundle" \
      "macos" \
      "${arch}" \
      "${version}" \
      "${worker_version}" \
      "${runtime_bundle}" \
      "worker_bundle" \
      "worker_bearer" \
      "${storage_relative_path}"
  done

  if [[ ${built_any} -eq 0 ]]; then
    printf 'No macOS embedding runtimes found. Set MANIFEED_MACOS_CPU_RUNTIME_DIR and/or MANIFEED_MACOS_COREML_RUNTIME_DIR.\n' >&2
    exit 1
  fi
}

update_catalog() {
  local metadata_path=$1
  local catalog_path=$2
  local download_base_url=$3
  local release_notes_base_url=$4
  local published_at=$5

  python3 - "${metadata_path}" "${catalog_path}" "${download_base_url}" "${release_notes_base_url}" "${published_at}" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

metadata_path = Path(sys.argv[1])
catalog_path = Path(sys.argv[2])
download_base_url = sys.argv[3].rstrip("/")
release_notes_base_url = sys.argv[4].rstrip("/")
published_at = sys.argv[5]

if catalog_path.exists():
    catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
else:
    catalog = {"items": []}

new_items = []
for raw_line in metadata_path.read_text(encoding="utf-8").splitlines():
    if not raw_line.strip():
        continue
    (
        absolute_path,
        family,
        product,
        platform,
        arch,
        version,
        worker_version,
        runtime_bundle,
        artifact_kind,
        download_auth,
        storage_relative_path,
    ) = raw_line.split("\t")
    artifact_path = Path(absolute_path)
    artifact_name = artifact_path.name
    item = {
        "artifact_name": artifact_name,
        "family": family,
        "product": product,
        "platform": platform,
        "arch": arch,
        "latest_version": version,
        "minimum_supported_version": version,
        "artifact_kind": artifact_kind,
        "sha256": hashlib.sha256(artifact_path.read_bytes()).hexdigest(),
        "download_auth": download_auth,
        "download_url": f"{download_base_url}/{artifact_name}",
        "release_notes_url": release_notes_base_url,
        "published_at": published_at,
        "storage_relative_path": storage_relative_path,
    }
    if worker_version.strip():
        item["worker_version"] = worker_version
    if runtime_bundle.strip():
        item["runtime_bundle"] = runtime_bundle
    new_items.append(item)

keys_to_replace = {
    (
        item["family"],
        item["product"],
        item["platform"],
        item["arch"],
        item.get("runtime_bundle"),
    )
    for item in new_items
}

preserved_items = []
for item in catalog.get("items", []):
    key = (
        item.get("family"),
        item["product"],
        item["platform"],
        item["arch"],
        item.get("runtime_bundle"),
    )
    if key in keys_to_replace:
        continue
    preserved_items.append(item)

catalog["items"] = preserved_items + new_items
catalog["items"].sort(
    key=lambda item: (
        item.get("family", ""),
        item["platform"],
        item["product"],
        item["arch"],
        item.get("runtime_bundle") or "",
    )
)
catalog_path.parent.mkdir(parents=True, exist_ok=True)
catalog_path.write_text(json.dumps(catalog, indent=2) + "\n", encoding="utf-8")
PY
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
      --storage-root)
        shift
        STORAGE_ROOT=${1:-}
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

  mkdir -p "${STORAGE_ROOT}"
  if [[ "${CATALOG_PATH}" == "${STORAGE_ROOT}/catalog.json" ]]; then
    :
  else
    mkdir -p "$(dirname "${CATALOG_PATH}")"
  fi

  METADATA_PATH=$(mktemp)
  trap 'rm -f "${METADATA_PATH:-}"' EXIT

  local platform arch
  platform=$(current_release_platform)
  arch=$(current_release_arch)

  local desktop_version rss_version rss_worker_version embedding_version embedding_worker_version
  desktop_version=$(resolve_artifact_version "${WORKERS_DIR}/worker-source-embedding-desktop/Cargo.toml" "${platform}" "${arch}")
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
    "$(normalize_url_base "${DOWNLOAD_BASE_URL}")" \
    "$(normalize_url_base "${RELEASE_NOTES_BASE_URL}")" \
    "${PUBLISHED_AT}"

  printf 'Artifacts stored under %s\n' "${STORAGE_ROOT}"
  printf 'Catalog updated: %s\n' "${CATALOG_PATH}"
}

main "$@"
