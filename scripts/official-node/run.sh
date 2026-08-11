#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STATE_DIR="${OFFICIAL_NODE_STATE_DIR:-${ROOT_DIR}/.local/official-node}"
HOME_DIR="${OFFICIAL_NODE_HOME:-${STATE_DIR}/home}"
JAR_PATH_FILE="${STATE_DIR}/jar-path"

if [[ ! -f "${JAR_PATH_FILE}" ]]; then
  echo "official node JAR is not built; run make official-build" >&2
  exit 1
fi

"${ROOT_DIR}/scripts/official-node/prepare-home.sh"

JAR_PATH="$(cat "${JAR_PATH_FILE}")"
if [[ ! -f "${JAR_PATH}" ]]; then
  echo "official node JAR no longer exists: ${JAR_PATH}" >&2
  exit 1
fi

export ALEPHIUM_HOME="${HOME_DIR}"
export ALEPHIUM_ENV=prod

exec java ${OFFICIAL_NODE_JAVA_OPTS:-} -jar "${JAR_PATH}"
