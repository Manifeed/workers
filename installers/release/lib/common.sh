#!/usr/bin/env bash

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'Missing required command: %s\n' "$1" >&2
    exit 1
  }
}

normalize_url_base() {
  printf '%s' "${1%/}"
}

default_public_base_url() {
  if [[ -n "${MANIFEED_PUBLIC_BASE_URL:-}" ]]; then
    normalize_url_base "${MANIFEED_PUBLIC_BASE_URL}"
    return
  fi

  local port=${EDGE_HTTP_PORT:-80}
  if [[ "${port}" == "80" ]]; then
    printf 'http://localhost'
    return
  fi

  printf 'http://localhost:%s' "${port}"
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

copy_file() {
  local source=$1
  local destination=$2
  install -d "$(dirname "${destination}")"
  cp -f "${source}" "${destination}"
}

find_linux_desktop_package() {
  local directory=$1
  local version=$2
  local deb_arch=$3
  find "${directory}" -maxdepth 1 -type f \
    -name "manifeed-workers-desktop_${version}-*_${deb_arch}.deb" \
    -printf '%p\n' 2>/dev/null \
    | sort -V \
    | tail -n 1
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
