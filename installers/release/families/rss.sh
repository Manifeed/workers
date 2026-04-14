#!/usr/bin/env bash

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
