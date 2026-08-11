#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STATE_DIR="${OFFICIAL_NODE_STATE_DIR:-${ROOT_DIR}/.local/official-node}"
HOME_DIR="${OFFICIAL_NODE_HOME:-${STATE_DIR}/home}"
CONFIG_SOURCE="${ROOT_DIR}/config/official-node/user.conf"
OFFICIAL_NODE_P2P_PORT="${OFFICIAL_NODE_P2P_PORT:-19985}"

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

if [[ ! "$OFFICIAL_NODE_P2P_PORT" =~ ^[0-9]+$ ]]; then
  echo "OFFICIAL_NODE_P2P_PORT must be a positive integer: ${OFFICIAL_NODE_P2P_PORT}" >&2
  exit 1
fi

if [[ "${OFFICIAL_NODE_P2P_PORT}" -eq 19973 || "${OFFICIAL_NODE_P2P_PORT}" -eq 19974 || "${OFFICIAL_NODE_P2P_PORT}" -eq 19975 ]]; then
  echo "Peringatan: ${OFFICIAL_NODE_P2P_PORT} sering dipakai untuk node Java/Alephium lokal. Gunakan port lain (mis. 19985) agar tidak bentrok." >&2
fi

if [[ "${OFFICIAL_NODE_P2P_PORT}" -ne 19985 ]]; then
  sed -i -E "s/(^\\s*bind-address\\s*=\\s*\"127\\.0\\.0\\.1:)[0-9]+(\\".*$)/\\1${OFFICIAL_NODE_P2P_PORT}\\2/" "${HOME_DIR}/user.conf"
  sed -i -E "s/(^\\s*internal-address\\s*=\\s*\"127\\.0\\.0\\.1:)[0-9]+(\\".*$)/\\1${OFFICIAL_NODE_P2P_PORT}\\2/" "${HOME_DIR}/user.conf"
  sed -i -E "s/(^\\s*coordinator-address\\s*=\\s*\"127\\.0\\.0\\.1:)[0-9]+(\\".*$)/\\1${OFFICIAL_NODE_P2P_PORT}\\2/" "${HOME_DIR}/user.conf"
fi

echo "Official node home: ${HOME_DIR}"
