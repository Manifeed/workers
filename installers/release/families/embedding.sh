#!/usr/bin/env bash

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
      build_embedding_bundle_linux \
        "${arch}" \
        "${version}" \
        "${worker_version}" \
        "${runtime_bundle}" \
        "${source}"
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
