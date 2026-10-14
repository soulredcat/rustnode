#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STATE_DIR="${OFFICIAL_NODE_STATE_DIR:-${ROOT_DIR}/.local/official-node}"
HOME_DIR="${OFFICIAL_NODE_HOME:-${STATE_DIR}/home}"
CONFIG_SOURCE="${ROOT_DIR}/config/official-node/user.conf"
OFFICIAL_NODE_P2P_PORT="${OFFICIAL_NODE_P2P_PORT:-19985}"
OFFICIAL_NODE_REST_PORT="${OFFICIAL_NODE_REST_PORT:-12973}"
OFFICIAL_NODE_MINER_API_PORT="${OFFICIAL_NODE_MINER_API_PORT:-10973}"

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

if [[ "${OFFICIAL_NODE_P2P_PORT}" -eq 19973 || "${OFFICIAL_NODE_P2P_PORT}" -eq 19974 || "${OFFICIAL_NODE_P2P_PORT}" -eq 19975 || "${OFFICIAL_NODE_P2P_PORT}" -eq 19985 ]]; then
  echo "Peringatan: ${OFFICIAL_NODE_P2P_PORT} sering dipakai untuk node Java/Alephium lokal. Jika bentrok, jalankan dengan OFFICIAL_NODE_P2P_PORT <port_lain> dan attach jika perlu." >&2
fi

if [[ "${OFFICIAL_NODE_P2P_PORT}" -ne 19985 ]]; then
  sed -i -E "s/(^\s*bind-address\s*=\s*\"127\.0\.0\.1:)[0-9]+(\".*$)/\1${OFFICIAL_NODE_P2P_PORT}\2/" "${HOME_DIR}/user.conf"
  sed -i -E "s/(^\s*internal-address\s*=\s*\"127\.0\.0\.1:)[0-9]+(\".*$)/\1${OFFICIAL_NODE_P2P_PORT}\2/" "${HOME_DIR}/user.conf"
  sed -i -E "s/(^\s*coordinator-address\s*=\s*\"127\.0\.0\.1:)[0-9]+(\".*$)/\1${OFFICIAL_NODE_P2P_PORT}\2/" "${HOME_DIR}/user.conf"
fi

if [[ ! "${OFFICIAL_NODE_REST_PORT}" =~ ^[0-9]+$ ]]; then
  echo "OFFICIAL_NODE_REST_PORT must be a positive integer: ${OFFICIAL_NODE_REST_PORT}" >&2
  exit 1
fi

if [[ ! "${OFFICIAL_NODE_MINER_API_PORT}" =~ ^[0-9]+$ ]]; then
  echo "OFFICIAL_NODE_MINER_API_PORT must be a positive integer: ${OFFICIAL_NODE_MINER_API_PORT}" >&2
  exit 1
fi

if [[ "${OFFICIAL_NODE_REST_PORT}" -ne 12973 || "${OFFICIAL_NODE_MINER_API_PORT}" -ne 10973 ]]; then
  sed -i -E "s/(^\s*rest-port\s*=\s*)[0-9]+(\s*)/\1${OFFICIAL_NODE_REST_PORT}\2/" "${HOME_DIR}/user.conf"
  sed -i -E "s/(^\s*miner-api-port\s*=\s*)[0-9]+(\s*)/\1${OFFICIAL_NODE_MINER_API_PORT}\2/" "${HOME_DIR}/user.conf"
fi

if [[ "${OFFICIAL_NODE_REST_PORT}" -eq 12973 || "${OFFICIAL_NODE_MINER_API_PORT}" -eq 10973 ]]; then
  echo "Peringatan: salah satu port resmi masih memakai default Alephium (12973/10973). Jika bentrok, set OFFICIAL_NODE_REST_PORT dan OFFICIAL_NODE_MINER_API_PORT." >&2
fi

echo "Official node home: ${HOME_DIR}"
