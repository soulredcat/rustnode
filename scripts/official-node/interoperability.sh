#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STATE_DIR="${OFFICIAL_NODE_STATE_DIR:-${ROOT_DIR}/.local/official-node}"
OFFICIAL_LOG_FILE="${STATE_DIR}/official-node.log"
OFFICIAL_PID_FILE="${STATE_DIR}/official-node.pid"
INTEROP_DIR="${RUST_INTEROP_DIR:-${ROOT_DIR}/tests/interoperability/fastpath}"
RUST_LOG_FILE="${INTEROP_DIR}/rust-node.log"
RUST_MANIFEST_FILE="${INTEROP_DIR}/commands.txt"
RUST_ERROR_FILE="${INTEROP_DIR}/error.txt"
RUST_PID_FILE="${INTEROP_DIR}/rust-node.pid"
ATTACH_TO_EXISTING="${RUST_INTEROP_ATTACH_TO_EXISTING:-0}"

DEFAULT_INTEROP_PORT="${RUST_INTEROP_PEER_PORT:-${OFFICIAL_NODE_P2P_PORT:-19985}}"
PEER_ADDRESS="${RUST_INTEROP_PEER_ADDRESS:-127.0.0.1:${DEFAULT_INTEROP_PORT}}"
PEER_HOST="${PEER_ADDRESS%:*}"
PEER_PORT="${PEER_ADDRESS##*:}"
DURATION_SECONDS="${RUST_INTEROP_DURATION_SECONDS:-120}"
OFFICIAL_READY_TIMEOUT="${OFFICIAL_NODE_STARTUP_TIMEOUT_SECONDS:-180}"
if [[ -z "${OFFICIAL_NODE_REST_PORT+x}" ]]; then
  if [[ "${PEER_PORT}" == "19985" ]]; then
    OFFICIAL_NODE_REST_PORT="12973"
  else
    OFFICIAL_NODE_REST_PORT="$((PEER_PORT - 7000))"
  fi
fi
if [[ -z "${OFFICIAL_NODE_MINER_API_PORT+x}" ]]; then
  if [[ "${PEER_PORT}" == "19985" ]]; then
    OFFICIAL_NODE_MINER_API_PORT="10973"
  else
    OFFICIAL_NODE_MINER_API_PORT="$((PEER_PORT - 9000))"
  fi
fi

API_BIND="${RUST_INTEROP_API_BIND:-127.0.0.1:22973}"
UPSTREAM_URL="http://${PEER_HOST}:${OFFICIAL_NODE_REST_PORT}"

OFFICIAL_CONFIG_FILE="${ROOT_DIR}/config/official-node/user.conf"
EXPECTED_RESULT="required: handshake (Hello), ping/pong exchange, block download (P2P v2), and API gateway passthrough"
ACTUAL_RESULT="in_progress"
OPERATOR_NOTES=""
NETWORK_ID="unknown"
P2P_VERSION="unknown"
OFFICIAL_REPOSITORY="unknown"

mkdir -p "${STATE_DIR}" "${INTEROP_DIR}"
: > "${RUST_ERROR_FILE}"

if [[ -f "${OFFICIAL_CONFIG_FILE}" ]]; then
  NETWORK_ID="$(sed -nE "s/^[[:space:]]*network-id[[:space:]]*=[[:space:]]*([0-9]+).*/\\1/p" "${OFFICIAL_CONFIG_FILE}" | tail -n 1)"
  if [[ "${NETWORK_ID}" == "" ]]; then
    NETWORK_ID="unknown"
  fi
  if grep -Eq "enable-p2p-v2[[:space:]]*=[[:space:]]*true" "${OFFICIAL_CONFIG_FILE}"; then
    P2P_VERSION="2"
  elif grep -Eq "enable-p2p-v2[[:space:]]*=[[:space:]]*false" "${OFFICIAL_CONFIG_FILE}"; then
    P2P_VERSION="1"
  fi
fi

if command -v git >/dev/null 2>&1; then
  OFFICIAL_REPOSITORY="$(git -C "${ROOT_DIR}/.upstream/alephium" remote get-url origin 2>/dev/null || echo unknown)"
  if [[ "${OFFICIAL_REPOSITORY}" == "" ]]; then
    OFFICIAL_REPOSITORY="unknown"
  fi
fi

is_port_open() {
  local host=$1
  local port=$2
  local ps_exe=""

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
    ps_exe="powershell.exe"
  elif [ -x "/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe" ]; then
    ps_exe="/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe"
  fi

  if [[ "${ps_exe}" != "" ]]; then
    "${ps_exe}" -NoProfile -NonInteractive -Command "try { (New-Object System.Net.Sockets.TcpClient('$host',$port)).Dispose(); exit 0 } catch { exit 1 }"
    return
  fi

  if (exec 9<>"/dev/tcp/${host}/${port}") >/dev/null 2>&1; then
    exec 9>&-
    exec 9<&-
    return 0
  fi
  return 1
}

append_operator_note() {
  local note=$1
  if [[ "${OPERATOR_NOTES}" == "" ]]; then
    OPERATOR_NOTES="${note}"
  else
    OPERATOR_NOTES="${OPERATOR_NOTES}; ${note}"
  fi
}

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
official_repository: ${OFFICIAL_REPOSITORY}
official_commit_or_release: ${official_commit}
official_build_command: ./scripts/official-node/build.sh
official_start_command: ${official_start_command}
official_listen_address: ${PEER_ADDRESS}
network_id: ${NETWORK_ID}
p2p_version: ${P2P_VERSION}
rust_commit: ${rust_commit}
rust_build_command: cargo build -p alephium-app
rust_start_command: cargo run -p alephium-app -- --api-bind ${API_BIND} --upstream ${UPSTREAM_URL} --route-mode address-group=native --network-id 2 ${PEER_ADDRESS}
test_duration_seconds: ${DURATION_SECONDS}
test_duration: ${DURATION_SECONDS}s
configuration_files: ${OFFICIAL_CONFIG_FILE},${ROOT_DIR}/scripts/official-node/run.sh,${ROOT_DIR}/scripts/official-node/prepare-home.sh
log_artifacts: ${RUST_LOG_FILE},${RUST_ERROR_FILE},${OFFICIAL_LOG_FILE},${RUST_MANIFEST_FILE}
packet_capture_artifact_if_any: none
expected_result: ${EXPECTED_RESULT}
actual_result: ${ACTUAL_RESULT}
operator_notes: ${OPERATOR_NOTES}
EOF
}

cleanup() {
  local code=$?
  if [[ "${ACTUAL_RESULT}" == "in_progress" ]]; then
    if [[ "${code}" -eq 0 ]]; then
      ACTUAL_RESULT="pass"
    else
      ACTUAL_RESULT="failed with exit ${code}"
    fi
  fi
  if [[ "${OPERATOR_NOTES}" == "" ]]; then
    append_operator_note "Interop run ended with code ${code}"
  fi
  write_manifest
  if [[ "${code}" -eq 0 ]]; then
    echo "Interop run completed successfully."
  fi

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

http_status() {
  curl -sS -m 5 -o /dev/null -w '%{http_code}' "$1" 2>/dev/null || echo "000"
}

# The gateway must pass allowlisted requests through unchanged and refuse the rest.
check_gateway() {
  local via_api="http://${API_BIND}"
  local direct_body via_body status

  if ! command -v curl >/dev/null 2>&1; then
    echo "curl not found; API gateway checks skipped" >&2
    append_operator_note "API gateway checks skipped: curl missing."
    return 0
  fi

  for _ in $(seq 1 30); do
    if [[ "$(http_status "${UPSTREAM_URL}/infos/node")" == "200" && "$(http_status "${via_api}/infos/node")" == "200" ]]; then
      break
    fi
    sleep 1
  done

  via_body="$(curl -sS -m 5 "${via_api}/infos/node" || true)"
  if [[ "${via_body}" != *releaseVersion* ]]; then
    echo "gateway GET /infos/node did not return official build info: ${via_body}" >&2
    return 1
  fi

  direct_body="$(curl -sS -m 5 "${UPSTREAM_URL}/mempool/transactions" || true)"
  via_body="$(curl -sS -m 5 "${via_api}/mempool/transactions" || true)"
  if [[ "$(http_status "${via_api}/mempool/transactions")" != "200" || "${direct_body}" != "${via_body}" ]]; then
    echo "gateway GET /mempool/transactions differs from the official node" >&2
    echo "direct: ${direct_body}" >&2
    echo "via gateway: ${via_body}" >&2
    return 1
  fi

  status="$(http_status "${via_api}/infos/self-clique")"
  if [[ "$(http_status "${UPSTREAM_URL}/infos/self-clique")" != "200" || "${status}" != "404" ]]; then
    echo "gateway did not refuse an endpoint outside the nixiaproject surface (got ${status})" >&2
    return 1
  fi

  # Native GET /addresses/{address}/group must answer exactly like the official
  # node: asset addresses natively, everything else (contract, grouped, invalid)
  # through the proxy. Addresses: official AddressSpec vectors (P2PKH, P2MPKH,
  # P2SH), devnet genesis addresses, a contract, a grouped and an invalid one.
  local address direct via
  for address in \
    1C2RAVWSuaXw8xtUxqVERR7ChKBE1XgscNFw73NSHE1v3 \
    1H7CmpbvGJwgyLzR91wzSJJSkiBC92WDPTWny4gmhQJQc \
    1DkrQMni2h8KYpvY8t7dECshL66gwnxiR5uD2Udxps6og \
    131R8ufDhcsu6SRztR9D3m8GUzkWFUPfT78aQ6jgtgzob \
    2jjvDdgGjC6X9HHMCMHohVfvp1uf3LHQrAGWaufR17P7AFwtxodTxSktqKc2urNEtaoUCy5xXpBUwpZ8QM8Q3e5BYCx \
    je9CrJD444xMSGDA2yr1XMvugoHuTc6pfYEaPYrKLuYa \
    rvpeCy7GhsGHq8n6TnB1LjQh4xn1FMHJVXnsdZAniKZA \
    17Ey9PHogBdGS5361s6WQeTXNG2KNFoEtdkJVJvM7KKwU \
    1GtpWTB3QEfHZg6Rfr5TCaxRRkLTXhDT1Dtk4Rm8kCPoc \
    17Tyyh8iG5JQkF7DFPeUXzPbK6JG4FW4erCmXHNA6a9qP \
    uqa8vVAySDGA5nbt6c5WXYXXuBWPghtDx6C8aE3VFXDr \
    1C2RAVWSuaXw8xtUxqVERR7ChKBE1XgscNFw73NSHE1v3:1 \
    not-an-address; do
    direct="$(curl -sS -m 5 -w ' %{http_code}' "${UPSTREAM_URL}/addresses/${address}/group" | tr -d ' \n\r\t' || true)"
    via="$(curl -sS -m 5 -w ' %{http_code}' "${via_api}/addresses/${address}/group" | tr -d ' \n\r\t' || true)"
    if [[ "${direct}" != "${via}" ]]; then
      echo "native address group differs for ${address}: official ${direct}, rustnode ${via}" >&2
      return 1
    fi
  done
  echo "native GET /addresses/{address}/group matches the official node"

  echo "API gateway passthrough verified against the official node"
  return 0
}

run_rust_peer() {
  if command -v cargo >/dev/null 2>&1; then
    (cd "${ROOT_DIR}" && cargo run -p alephium-app -- --api-bind "${API_BIND}" --upstream "${UPSTREAM_URL}" --route-mode address-group=native --network-id 2 "${PEER_ADDRESS}")
    return
  fi

  if command -v powershell.exe >/dev/null 2>&1; then
    local windows_root
    windows_root="$(wslpath -w "${ROOT_DIR}")"
    powershell.exe -NoProfile -NonInteractive -Command "& { Set-Location -LiteralPath '${windows_root}'; cargo run -p alephium-app -- --api-bind ${API_BIND} --upstream ${UPSTREAM_URL} --route-mode address-group=native --network-id 2 ${PEER_ADDRESS} }"
    return
  fi

  echo "cargo not found; please install Rust or make cargo available in shell PATH" >&2
  return 1
}

if [[ "${ATTACH_TO_EXISTING}" -eq 0 ]]; then
  if is_port_open "${PEER_HOST}" "${PEER_PORT}"; then
    echo "WARNING: ${PEER_ADDRESS} already has a listening process." >&2
    echo "Jika node Java kamu sedang jalan di port ini, pakai RUST_INTEROP_ATTACH_TO_EXISTING=1 atau pindah port dengan RUST_INTEROP_PEER_ADDRESS." >&2
    append_operator_note "startup blocked: ${PEER_ADDRESS} already listening."
    exit 1
  fi

  "${ROOT_DIR}/scripts/official-node/build.sh"
  OFFICIAL_NODE_RESET=1 \
  OFFICIAL_NODE_REST_PORT="${OFFICIAL_NODE_REST_PORT}" \
  OFFICIAL_NODE_MINER_API_PORT="${OFFICIAL_NODE_MINER_API_PORT}" \
  "${ROOT_DIR}/scripts/official-node/prepare-home.sh"

  (
    "${ROOT_DIR}/scripts/official-node/run.sh"
  ) > "${OFFICIAL_LOG_FILE}" 2>&1 &
  official_pid=$!
  printf '%s\n' "${official_pid}" > "${OFFICIAL_PID_FILE}"

  official_ready=0
  for _ in $(seq 1 "${OFFICIAL_READY_TIMEOUT}"); do
    if ! kill -0 "${official_pid}" 2>/dev/null; then
      echo "official node exited before readiness" >&2
      append_operator_note "official node exited before readiness check."
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
    append_operator_note "official node failed to open ${PEER_ADDRESS} within timeout."
    cat "${OFFICIAL_LOG_FILE}" >&2
    exit 1
  fi
else
  if ! is_port_open "${PEER_HOST}" "${PEER_PORT}"; then
    echo "no peer listening at ${PEER_ADDRESS}; start one externally or run without RUST_INTEROP_ATTACH_TO_EXISTING" >&2
    append_operator_note "ATTACH mode requested but peer not reachable at ${PEER_ADDRESS}."
    exit 1
  fi
  echo "attaching to existing peer at ${PEER_ADDRESS}; script tidak akan menyalakan atau mematikan node official/Java."
  append_operator_note "ATTACH mode; official node lifecycle not managed."
fi

(
  run_rust_peer
) >"${RUST_LOG_FILE}" 2>"${RUST_ERROR_FILE}" &
rust_pid=$!
printf '%s\n' "${rust_pid}" > "${RUST_PID_FILE}"

deadline=$(( $(date +%s) + DURATION_SECONDS ))
handshake_seen=0
ping_seen=0
blocks_seen=0

while true; do
  now=$(date +%s)

  if [[ "${ATTACH_TO_EXISTING}" -eq 0 ]]; then
    if ! kill -0 "${official_pid}" 2>/dev/null; then
      echo "official node died during interop run" >&2
      append_operator_note "official node died during run."
      cat "${OFFICIAL_LOG_FILE}" >&2
      exit 1
    fi
  fi

  if ! kill -0 "${rust_pid}" 2>/dev/null; then
    echo "rust process exited during interop run" >&2
    append_operator_note "rust process exited early before deadline."
    cat "${RUST_ERROR_FILE}" >&2
    if (( now >= deadline )); then
      break
    fi
    exit 1
  fi

  if [[ -f "${RUST_LOG_FILE}" ]]; then
    if grep -q "received valid Hello" "${RUST_LOG_FILE}"; then
      handshake_seen=1
    fi
    if grep -q "received matching pong" "${RUST_LOG_FILE}"; then
      ping_seen=1
    fi
    if grep -q "received BlocksAndUnclesByHeightsResponse" "${RUST_LOG_FILE}"; then
      blocks_seen=1
    fi
  fi

  if (( now >= deadline )); then
    break
  fi

  sleep 1
done

if [[ "${handshake_seen}" -ne 1 || "${ping_seen}" -ne 1 || "${blocks_seen}" -ne 1 ]]; then
  echo "interop assertions not observed before timeout" >&2
  append_operator_note "handshake+ping assertions not observed before deadline."
  cat "${RUST_LOG_FILE}" >&2
  cat "${RUST_ERROR_FILE}" >&2
  exit 1
fi

echo "interop window reached with Hello, Ping/Pong, and P2P v2 block download observed"

if ! check_gateway; then
  append_operator_note "API gateway checks failed."
  cat "${RUST_ERROR_FILE}" >&2
  exit 1
fi

# Block encoding and JSON, checked against every block the official node holds.
if ! (cd "${ROOT_DIR}" && RUSTNODE_OFFICIAL_URL="${UPSTREAM_URL}" \
  cargo test -p alephium-model --test official_raw_blocks -- --ignored --nocapture &&
  RUSTNODE_OFFICIAL_URL="${UPSTREAM_URL}" \
  cargo test -p alephium-api --test official_block_json -- --ignored --nocapture &&
  RUSTNODE_OFFICIAL_URL="${UPSTREAM_URL}" \
  cargo test -p alephium-chain --test official_chain -- --ignored --nocapture); then
  append_operator_note "block encoding or JSON differs from the official node."
  exit 1
fi
echo "block encoding and JSON verified against the official node"
