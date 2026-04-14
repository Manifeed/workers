#!/usr/bin/env bash

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

update_catalog() {
  local metadata_path=$1
  local catalog_path=$2
  local download_base_url=$3
  local release_notes_base_url=$4
  local published_at=$5

  python3 - "${metadata_path}" "${catalog_path}" "${download_base_url}" "${release_notes_base_url}" "${published_at}" <<'PY'
import hashlib
import json
import re
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

version_pattern = re.compile(r"(\d+\.\d+\.\d+)")

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

def release_identity(item: dict) -> tuple:
    return (
        item.get("family"),
        item.get("product"),
        item.get("platform"),
        item.get("arch"),
        item.get("runtime_bundle"),
        item.get("artifact_name")
        or item.get("storage_relative_path")
        or item.get("download_url"),
    )


def normalize_preserved_item(item: dict) -> dict:
    artifact_name = item.get("artifact_name") or ""
    match = version_pattern.search(artifact_name)
    if match is None:
        return item

    normalized = dict(item)
    normalized["latest_version"] = match.group(1)
    normalized["minimum_supported_version"] = match.group(1)
    return normalized

new_identities = {release_identity(item) for item in new_items}

preserved_items = []
for item in catalog.get("items", []):
    if release_identity(item) in new_identities:
        continue
    preserved_items.append(normalize_preserved_item(item))

catalog["items"] = preserved_items + new_items
catalog["items"].sort(
    key=lambda item: (
        item.get("family", ""),
        item["platform"],
        item["product"],
        item["arch"],
        item.get("runtime_bundle") or "",
        item.get("latest_version", ""),
        item.get("artifact_name", ""),
    )
)
catalog_path.parent.mkdir(parents=True, exist_ok=True)
catalog_path.write_text(json.dumps(catalog, indent=2) + "\n", encoding="utf-8")
PY
}
