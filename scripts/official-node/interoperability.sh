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
ATTACH_TO_EXISTING="${RUST_INTEROP_ATTACH_TO_EXISTING:-0}"

DEFAULT_INTEROP_PORT="${RUST_INTEROP_PEER_PORT:-${OFFICIAL_NODE_P2P_PORT:-19974}}"
PEER_ADDRESS="${RUST_INTEROP_PEER_ADDRESS:-127.0.0.1:${DEFAULT_INTEROP_PORT}}"
PEER_HOST="${PEER_ADDRESS%:*}"
PEER_PORT="${PEER_ADDRESS##*:}"
DURATION_SECONDS="${RUST_INTEROP_DURATION_SECONDS:-120}"
OFFICIAL_READY_TIMEOUT="${OFFICIAL_NODE_STARTUP_TIMEOUT_SECONDS:-180}"
REQUIRE_M2="${RUST_INTEROP_REQUIRE_M2:-0}"

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

  if command -v powershell.exe >/dev/null 2>&1; then
    powershell.exe -NoProfile -NonInteractive -Command "try { (New-Object System.Net.Sockets.TcpClient('$host',$port)).Dispose(); exit 0 } catch { exit 1 }"
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

  if [[ "${ATTACH_TO_EXISTING}" -eq 0 && -f "${OFFICIAL_PID_FILE}" ]]; then
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

write_manifest() {
  local official_start_command="attached to existing peer at ${PEER_ADDRESS}"
  local official_commit="unknown"
  local rust_commit="unknown"

  if [[ "${ATTACH_TO_EXISTING}" -eq 0 ]]; then
    official_start_command="${ROOT_DIR}/scripts/official-node/run.sh"
  fi

  if command -v git >/dev/null 2>&1; then
    official_commit="$(git -C "${ROOT_DIR}/.upstream/alephium" rev-parse HEAD 2>/dev/null || echo unknown)"
    rust_commit="$(git -C "${ROOT_DIR}" rev-parse HEAD 2>/dev/null || echo unknown)"
  fi

  cat > "${RUST_MANIFEST_FILE}" <<EOF
run_id: ${RANDOM}
date_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)
official_commit_or_release: ${official_commit}
rust_commit: ${rust_commit}
rust_build_command: cargo build -p alephium-app
rust_start_command: cargo run -p alephium-app -- ${PEER_ADDRESS}
official_start_command: ${official_start_command}
official_listen_address: ${PEER_ADDRESS}
test_duration_seconds: ${DURATION_SECONDS}
environment_file: ${RUST_MANIFEST_FILE}
EOF
}

run_rust_peer() {
  if command -v cargo >/dev/null 2>&1; then
    (cd "${ROOT_DIR}" && cargo run -p alephium-app -- "${PEER_ADDRESS}")
    return
  fi

  if command -v powershell.exe >/dev/null 2>&1; then
    local windows_root
    windows_root="$(wslpath -w "${ROOT_DIR}")"
    powershell.exe -NoProfile -NonInteractive -Command "& { Set-Location -LiteralPath '${windows_root}'; cargo run -p alephium-app -- ${PEER_ADDRESS} }"
    return
  fi

  echo "cargo not found; please install Rust or make cargo available in shell PATH" >&2
  return 1
}

mkdir -p "${STATE_DIR}" "${INTEROP_DIR}"
write_manifest

if [[ "${ATTACH_TO_EXISTING}" -eq 0 ]]; then
  if is_port_open "${PEER_HOST}" "${PEER_PORT}"; then
    echo "WARNING: ${PEER_ADDRESS} already has a listening process." >&2
    echo "Jika Java node Anda sedang jalan di port ini, gunakan ATTACH mode agar tidak bentrok." >&2
    echo "Lanjutkan hanya dengan membunuh proses yang dipakai port itu atau pindah ke port lain dengan RUST_INTEROP_PEER_ADDRESS." >&2
    exit 1
  fi

  "${ROOT_DIR}/scripts/official-node/build.sh"
  OFFICIAL_NODE_RESET=1 "${ROOT_DIR}/scripts/official-node/prepare-home.sh"

  (
    "${ROOT_DIR}/scripts/official-node/run.sh"
  ) > "${OFFICIAL_LOG_FILE}" 2>&1 &
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
else
  if [[ "${PEER_PORT}" -eq 19973 ]]; then
    echo "Peringatan: port 19973 biasanya dipakai oleh node Java default. Default interop sekarang 19974." >&2
  fi
  if ! is_port_open "${PEER_HOST}" "${PEER_PORT}"; then
    echo "no peer listening at ${PEER_ADDRESS}; start one externally or run without RUST_INTEROP_ATTACH_TO_EXISTING" >&2
    exit 1
  fi
  echo "attaching to existing peer at ${PEER_ADDRESS} (skip official node startup/cleanup)"
  echo "WARNING: ATTACH mode aktif, script tidak akan menyalakan atau mematikan node Java/official." >&2
fi

(
  run_rust_peer
) >"${RUST_LOG_FILE}" 2>"${RUST_ERROR_FILE}" &
rust_pid=$!
printf '%s\n' "${rust_pid}" > "${RUST_PID_FILE}"

deadline=$(( $(date +%s) + DURATION_SECONDS ))
handshake_seen=0
ping_seen=0
m2_inv_request_seen=0
m2_headers_request_seen=0
m2_blocks_request_seen=0
m2_txs_request_seen=0
m2_headers_response_seen=0
m2_blocks_response_seen=0
m2_txs_response_seen=0

while true; do
  now=$(date +%s)

  if [[ "${ATTACH_TO_EXISTING}" -eq 0 ]]; then
    if ! kill -0 "${official_pid}" 2>/dev/null; then
      echo "official node died during interop run" >&2
      cat "${OFFICIAL_LOG_FILE}" >&2
      exit 1
    fi
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
    if grep -q "sent InvRequest" "${RUST_LOG_FILE}"; then
      m2_inv_request_seen=1
    fi
    if grep -q "sent HeadersRequest" "${RUST_LOG_FILE}"; then
      m2_headers_request_seen=1
    fi
    if grep -q "sent BlocksRequest" "${RUST_LOG_FILE}"; then
      m2_blocks_request_seen=1
    fi
    if grep -q "sent TxsRequest" "${RUST_LOG_FILE}"; then
      m2_txs_request_seen=1
    fi
    if grep -q "received HeadersResponse" "${RUST_LOG_FILE}"; then
      m2_headers_response_seen=1
    fi
    if grep -q "received BlocksResponse" "${RUST_LOG_FILE}"; then
      m2_blocks_response_seen=1
    fi
    if grep -q "received TxsResponse" "${RUST_LOG_FILE}"; then
      m2_txs_response_seen=1
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

if [[ "${REQUIRE_M2}" -eq 1 ]]; then
  if [[ "${m2_inv_request_seen}" -ne 1 || "${m2_headers_request_seen}" -ne 1 || "${m2_blocks_request_seen}" -ne 1 || "${m2_txs_request_seen}" -ne 1 ]]; then
    echo "M2 request markers not all observed before timeout" >&2
    cat "${RUST_LOG_FILE}" >&2
    cat "${RUST_ERROR_FILE}" >&2
    exit 1
  fi
  if [[ "${m2_headers_response_seen}" -ne 1 || "${m2_blocks_response_seen}" -ne 1 || "${m2_txs_response_seen}" -ne 1 ]]; then
    echo "M2 response markers not all observed before timeout" >&2
    cat "${RUST_LOG_FILE}" >&2
    cat "${RUST_ERROR_FILE}" >&2
    exit 1
  fi
fi

echo "Interop run completed successfully."
