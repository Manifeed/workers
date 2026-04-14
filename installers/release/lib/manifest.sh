#!/usr/bin/env bash

MANIFEST_HELPER="${WORKERS_DIR}/installers/release/read_manifest_value.py"

manifest_value() {
  local manifest_path=$1
  local key_path=$2
  local default_value=${3:-}
  python3 "${MANIFEST_HELPER}" "${manifest_path}" "${key_path}" "${default_value}"
}

resolve_package_version() {
  local manifest_path=$1
  manifest_value "${manifest_path}" "package.version"
}

resolve_artifact_version() {
  local manifest_path=$1
  local platform=$2
  local arch=$3
  local normalized_platform normalized_arch override_key override_value

  normalized_platform=$(printf '%s' "${platform}" | tr '-' '_')
  normalized_arch=$(printf '%s' "${arch}" | tr '-' '_')
  override_key="package.metadata.manifeed.release.artifact_version_${normalized_platform}_${normalized_arch}"
  override_value=$(manifest_value "${manifest_path}" "${override_key}")
  if [[ -n "${override_value}" ]]; then
    printf '%s\n' "${override_value}"
    return
  fi
  resolve_package_version "${manifest_path}"
}

resolve_worker_version_metadata() {
  local manifest_path=$1
  manifest_value "${manifest_path}" "package.metadata.manifeed.release.worker_version"
}
