#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STATE_DIR="${OFFICIAL_NODE_STATE_DIR:-${ROOT_DIR}/.local/official-node}"
HOME_DIR="${OFFICIAL_NODE_HOME:-${STATE_DIR}/home}"
CONFIG_SOURCE="${ROOT_DIR}/config/official-node/user.conf"

case "${HOME_DIR}" in
  "${ROOT_DIR}"/*) ;;
  *)
    echo "refusing to manage official-node home outside repository root: ${HOME_DIR}" >&2
    exit 1
    ;;
esac

if [[ "${OFFICIAL_NODE_RESET:-0}" == "1" ]]; then
  rm -rf "${HOME_DIR}"
fi

mkdir -p "${HOME_DIR}"
install -m 0600 "${CONFIG_SOURCE}" "${HOME_DIR}/user.conf"

echo "Official node home: ${HOME_DIR}"
