#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="${ALEPHIUM_UPSTREAM_DIR:-${ROOT_DIR}/.upstream/alephium}"
REPOSITORY="https://github.com/alephium/alephium.git"
COMMIT="63bcf6998508025f9538f3377629fe9e66df6c53"
GIT_CMD="${GIT_CMD:-git}"

mkdir -p "$(dirname "${TARGET_DIR}")"

if [[ ! -d "${TARGET_DIR}/.git" ]]; then
  "${GIT_CMD}" clone --filter=blob:none "${REPOSITORY}" "${TARGET_DIR}"
fi

"${GIT_CMD}" -C "${TARGET_DIR}" fetch --force origin "${COMMIT}"
"${GIT_CMD}" -C "${TARGET_DIR}" checkout --detach "${COMMIT}"

ACTUAL_COMMIT="$("${GIT_CMD}" -C "${TARGET_DIR}" rev-parse HEAD)"
if [[ "${ACTUAL_COMMIT}" != "${COMMIT}" ]]; then
  echo "upstream verification failed: expected ${COMMIT}, got ${ACTUAL_COMMIT}" >&2
  exit 1
fi

echo "Alephium upstream ready: ${TARGET_DIR} @ ${ACTUAL_COMMIT}"
