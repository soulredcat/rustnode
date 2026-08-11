#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STATE_DIR="${OFFICIAL_NODE_STATE_DIR:-${ROOT_DIR}/.local/official-node}"
LOG_FILE="${STATE_DIR}/official-node.log"
PID_FILE="${STATE_DIR}/official-node.pid"
P2P_HOST="127.0.0.1"
P2P_PORT="${OFFICIAL_NODE_P2P_PORT:-19975}"
STARTUP_TIMEOUT_SECONDS="${OFFICIAL_NODE_STARTUP_TIMEOUT_SECONDS:-180}"

mkdir -p "${STATE_DIR}"
OFFICIAL_NODE_RESET=1 "${ROOT_DIR}/scripts/official-node/prepare-home.sh"

cleanup() {
  if [[ -f "${PID_FILE}" ]]; then
    pid="$(cat "${PID_FILE}")"
    if kill -0 "${pid}" 2>/dev/null; then
      kill "${pid}" 2>/dev/null || true
      for _ in $(seq 1 20); do
        kill -0 "${pid}" 2>/dev/null || break
        sleep 0.25
      done
      kill -9 "${pid}" 2>/dev/null || true
    fi
    rm -f "${PID_FILE}"
  fi
}
trap cleanup EXIT INT TERM

is_port_open() {
  local host=$1
  local port=$2

  if command -v python3 >/dev/null 2>&1; then
    python3 - "${host}" "${port}" <<'PY'
import socket
import sys

host = sys.argv[1]
port = int(sys.argv[2])
with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
    sock.settimeout(0.5)
    sys.exit(0 if sock.connect_ex((host, port)) == 0 else 1)
PY
    return
  fi

  if command -v python >/dev/null 2>&1; then
    python - "$host" "$port" <<'PY'
import socket
import sys

host = sys.argv[1]
port = int(sys.argv[2])
with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
    sock.settimeout(0.5)
    sys.exit(0 if sock.connect_ex((host, port)) == 0 else 1)
PY
    return
  fi

  if (exec 9<>"/dev/tcp/${host}/${port}") >/dev/null 2>&1; then
    exec 9>&-
    exec 9<&-
    return 0
  fi

  return 1
}

if is_port_open "${P2P_HOST}" "${P2P_PORT}"; then
  echo "Peringatan: ${P2P_HOST}:${P2P_PORT} sudah dipakai proses lain (kemungkinan Java/Alephium)." >&2
  echo "Jalankan smoke dengan OFFICIAL_NODE_P2P_PORT pada port bebas agar tidak konflik." >&2
  echo "Untuk co-location, default aman untuk devnet yang sama-sama lokal adalah 19975." >&2
  exit 1
fi

if [[ "${P2P_PORT}" -eq 19973 || "${P2P_PORT}" -eq 19974 ]]; then
  echo "Peringatan: port ${P2P_PORT} sering dipakai node Java; gunakan OFFICIAL_NODE_P2P_PORT=19975 agar tidak bentrok." >&2
fi

"${ROOT_DIR}/scripts/official-node/build.sh"

: > "${LOG_FILE}"
(
  "${ROOT_DIR}/scripts/official-node/run.sh"
) >"${LOG_FILE}" 2>&1 &
pid=$!
printf '%s\n' "${pid}" > "${PID_FILE}"

ready=0
for _ in $(seq 1 "${STARTUP_TIMEOUT_SECONDS}"); do
  if ! kill -0 "${pid}" 2>/dev/null; then
    echo "official node exited before opening P2P port" >&2
    cat "${LOG_FILE}" >&2
    exit 1
  fi

  if is_port_open "${P2P_HOST}" "${P2P_PORT}"; then
    ready=1
    break
  fi

  sleep 1
done

if [[ "${ready}" -ne 1 ]]; then
  echo "official node did not open ${P2P_HOST}:${P2P_PORT} within ${STARTUP_TIMEOUT_SECONDS}s" >&2
  cat "${LOG_FILE}" >&2
  exit 1
fi

sleep 3
if ! kill -0 "${pid}" 2>/dev/null; then
  echo "official node exited immediately after readiness" >&2
  cat "${LOG_FILE}" >&2
  exit 1
fi

echo "Official Alephium node is listening on ${P2P_HOST}:${P2P_PORT}"
tail -n 80 "${LOG_FILE}"
