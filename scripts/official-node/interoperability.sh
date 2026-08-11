#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STATE_DIR="${OFFICIAL_NODE_STATE_DIR:-${ROOT_DIR}/.local/official-node}"
OFFICIAL_LOG_FILE="${STATE_DIR}/official-node.log"
OFFICIAL_PID_FILE="${STATE_DIR}/official-node.pid"
INTEROP_DIR="${RUST_INTEROP_DIR:-${ROOT_DIR}/tests/interoperability/m1}"
RUST_LOG_FILE="${INTEROP_DIR}/rust-node.log"
RUST_MANIFEST_FILE="${INTEROP_DIR}/commands.txt"
RUST_ERROR_FILE="${INTEROP_DIR}/error.txt"
RUST_PID_FILE="${INTEROP_DIR}/rust-node.pid"

PEER_ADDRESS="${RUST_INTEROP_PEER_ADDRESS:-127.0.0.1:19973}"
PEER_HOST="${PEER_ADDRESS%:*}"
PEER_PORT="${PEER_ADDRESS##*:}"
DURATION_SECONDS="${RUST_INTEROP_DURATION_SECONDS:-120}"
OFFICIAL_READY_TIMEOUT="${OFFICIAL_NODE_STARTUP_TIMEOUT_SECONDS:-180}"

: > "${RUST_ERROR_FILE}"

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

cleanup() {
  local code=$?

  if [[ -f "${RUST_PID_FILE}" ]]; then
    rust_pid="$(cat "${RUST_PID_FILE}")"
    if kill -0 "${rust_pid}" 2>/dev/null; then
      kill "${rust_pid}" 2>/dev/null || true
    fi
    rm -f "${RUST_PID_FILE}"
  fi

  if [[ -f "${OFFICIAL_PID_FILE}" ]]; then
    pid="$(cat "${OFFICIAL_PID_FILE}")"
    if kill -0 "${pid}" 2>/dev/null; then
      kill "${pid}" 2>/dev/null || true
      for _ in $(seq 1 20); do
        kill -0 "${pid}" 2>/dev/null || break
        sleep 0.25
      done
      kill -9 "${pid}" 2>/dev/null || true
    fi
    rm -f "${OFFICIAL_PID_FILE}"
  fi

  exit "${code}"
}
trap cleanup EXIT INT TERM

mkdir -p "${STATE_DIR}" "${INTEROP_DIR}"

write_manifest() {
  cat > "${RUST_MANIFEST_FILE}" <<EOF
run_id: ${RANDOM}
date_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)
official_commit_or_release: $(git -C "${ROOT_DIR}/.upstream/alephium" rev-parse HEAD 2>/dev/null || echo unknown)
rust_commit: $(git -C "${ROOT_DIR}" rev-parse HEAD)
rust_build_command: cargo build -p alephium-app
rust_start_command: cargo run -p alephium-app -- ${PEER_ADDRESS}
official_start_command: ${ROOT_DIR}/scripts/official-node/run.sh
official_listen_address: ${PEER_ADDRESS}
test_duration_seconds: ${DURATION_SECONDS}
environment_file: ${RUST_MANIFEST_FILE}
EOF
}

"${ROOT_DIR}/scripts/official-node/build.sh"
write_manifest

OFFICIAL_NODE_RESET=1 "${ROOT_DIR}/scripts/official-node/prepare-home.sh"

(
  "${ROOT_DIR}/scripts/official-node/run.sh"
) >"${OFFICIAL_LOG_FILE}" 2>&1 &
official_pid=$!
printf '%s\n' "${official_pid}" > "${OFFICIAL_PID_FILE}"

official_ready=0
for _ in $(seq 1 "${OFFICIAL_READY_TIMEOUT}"); do
  if ! kill -0 "${official_pid}" 2>/dev/null; then
    echo "official node exited before readiness" >&2
    cat "${OFFICIAL_LOG_FILE}" >&2
    exit 1
  fi

  if is_port_open "${PEER_HOST}" "${PEER_PORT}"; then
    official_ready=1
    break
  fi
  sleep 1
done

if [[ "${official_ready}" -ne 1 ]]; then
  echo "official node did not open ${PEER_ADDRESS} within ${OFFICIAL_READY_TIMEOUT}s" >&2
  cat "${OFFICIAL_LOG_FILE}" >&2
  exit 1
fi

(
  cd "${ROOT_DIR}"
  cargo run -p alephium-app -- "${PEER_ADDRESS}"
) >"${RUST_LOG_FILE}" 2>"${RUST_ERROR_FILE}" &
rust_pid=$!
printf '%s\n' "${rust_pid}" > "${RUST_PID_FILE}"

deadline=$(( $(date +%s) + DURATION_SECONDS ))
handshake_seen=0
ping_seen=0

while true; do
  now=$(date +%s)
  if ! kill -0 "${official_pid}" 2>/dev/null; then
    echo "official node died during interop run" >&2
    cat "${OFFICIAL_LOG_FILE}" >&2
    exit 1
  fi

  if ! kill -0 "${rust_pid}" 2>/dev/null; then
    echo "rust process exited during interop run" >&2
    cat "${RUST_ERROR_FILE}" >&2
    if (( now >= deadline )); then
      break
    fi
    exit 1
  fi

  if [[ ! -f "${RUST_LOG_FILE}" ]]; then
    :
  else
    if grep -q "received valid Hello" "${RUST_LOG_FILE}"; then
      handshake_seen=1
    fi
    if grep -q "received matching pong" "${RUST_LOG_FILE}"; then
      ping_seen=1
    fi
  fi

  if [[ "${handshake_seen}" -eq 1 && "${ping_seen}" -eq 1 ]]; then
    if (( now >= deadline )); then
      echo "interop window reached with Hello+Ping/Pong observed"
      break
    fi
  fi

  if (( now >= deadline )); then
    break
  fi

  sleep 1
done

if [[ "${handshake_seen}" -ne 1 || "${ping_seen}" -ne 1 ]]; then
  echo "interop assertions not yet observed before timeout" >&2
  cat "${RUST_LOG_FILE}" >&2
  cat "${RUST_ERROR_FILE}" >&2
  exit 1
fi

echo "Interop run completed successfully."
