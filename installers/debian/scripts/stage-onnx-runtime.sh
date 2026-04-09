#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 3 ]; then
  printf 'Usage: %s <cpu|cuda12> <amd64|arm64> <destination>\n' "$0" >&2
  exit 1
fi

BUNDLE=$1
ARCH=$2
DESTINATION=$3

ORT_VERSION="1.24.2"
CPU_X64_URL="https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-x64-${ORT_VERSION}.tgz"
CPU_X64_SHA256="43725474ba5663642e17684717946693850e2005efbd724ac72da278fead25e6"
CUDA_X64_URL="https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-x64-gpu-${ORT_VERSION}.tgz"
CUDA_X64_SHA256="bcb42da041f42192e5579de175f7410313c114740a611e230afe9d79be65cc49"
CPU_ARM64_URL="https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-aarch64-${ORT_VERSION}.tgz"
CPU_ARM64_SHA256="6715b3d19965a2a6981e78ed4ba24f17a8c30d2d26420dbed10aac7ceca0085e"

case "${BUNDLE}:${ARCH}" in
  cpu:amd64)
    DOWNLOAD_URL="${CPU_X64_URL}"
    EXPECTED_SHA256="${CPU_X64_SHA256}"
    BUNDLE_MARKER="none"
    ;;
  cpu:arm64)
    DOWNLOAD_URL="${CPU_ARM64_URL}"
    EXPECTED_SHA256="${CPU_ARM64_SHA256}"
    BUNDLE_MARKER="none"
    ;;
  cuda12:amd64)
    DOWNLOAD_URL="${CUDA_X64_URL}"
    EXPECTED_SHA256="${CUDA_X64_SHA256}"
    BUNDLE_MARKER="cuda12"
    ;;
  *)
    printf 'Unsupported runtime bundle %s for architecture %s\n' "${BUNDLE}" "${ARCH}" >&2
    exit 1
    ;;
esac

WORK_DIR=$(mktemp -d)
cleanup() {
  rm -rf "${WORK_DIR}"
}
trap cleanup EXIT

ARCHIVE_PATH="${WORK_DIR}/runtime.tgz"
EXTRACT_DIR="${WORK_DIR}/extract"

mkdir -p "${EXTRACT_DIR}" "${DESTINATION}/lib"
curl -L --fail --show-error --output "${ARCHIVE_PATH}" "${DOWNLOAD_URL}"
printf '%s  %s\n' "${EXPECTED_SHA256}" "${ARCHIVE_PATH}" | sha256sum --check --status
tar -xzf "${ARCHIVE_PATH}" -C "${EXTRACT_DIR}"

LIB_DIR=$(find "${EXTRACT_DIR}" -maxdepth 3 -type d -path '*/lib' | head -n 1)
[ -n "${LIB_DIR}" ] || {
  printf 'Unable to locate extracted ONNX Runtime lib directory\n' >&2
  exit 1
}

rm -rf "${DESTINATION}/lib"
mkdir -p "${DESTINATION}/lib"
cp -a "${LIB_DIR}/." "${DESTINATION}/lib/"
printf '%s\n' "${BUNDLE_MARKER}" > "${DESTINATION}/bundle.txt"

if [ "${BUNDLE}" = "cuda12" ]; then
  rm -f "${DESTINATION}/lib"/libonnxruntime_providers_tensorrt.so*
fi

[ -f "${DESTINATION}/lib/libonnxruntime.so" ] || {
  printf 'libonnxruntime.so missing after runtime staging\n' >&2
  exit 1
}
