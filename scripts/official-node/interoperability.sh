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

DEFAULT_INTEROP_PORT="${RUST_INTEROP_PEER_PORT:-${OFFICIAL_NODE_P2P_PORT:-19985}}"
PEER_ADDRESS="${RUST_INTEROP_PEER_ADDRESS:-127.0.0.1:${DEFAULT_INTEROP_PORT}}"
PEER_HOST="${PEER_ADDRESS%:*}"
PEER_PORT="${PEER_ADDRESS##*:}"
DURATION_SECONDS="${RUST_INTEROP_DURATION_SECONDS:-120}"
OFFICIAL_READY_TIMEOUT="${OFFICIAL_NODE_STARTUP_TIMEOUT_SECONDS:-180}"
REQUIRE_M2="${RUST_INTEROP_REQUIRE_M2:-0}"
REQUIRE_M2_ANNOUNCEMENTS="${RUST_INTEROP_REQUIRE_M2_ANNOUNCEMENTS:-0}"
ANNOUNCEMENTS_CHECK=0
ANNOUNCEMENTS_REQUIRED=0
ANNOUNCEMENT_MODE="off"
OFFICIAL_NODE_AUTO_MINE_FOR_DEV="${OFFICIAL_NODE_AUTO_MINE_FOR_DEV:-0}"
if [[ -z "${OFFICIAL_NODE_REST_PORT+x}" ]]; then
  if [[ "${PEER_PORT}" == "19985" ]]; then
    OFFICIAL_NODE_REST_PORT="12973"
  else
    OFFICIAL_NODE_REST_PORT="$((PEER_PORT - 7000))"
  fi
else
  OFFICIAL_NODE_REST_PORT="${OFFICIAL_NODE_REST_PORT}"
fi
if [[ -z "${OFFICIAL_NODE_MINER_API_PORT+x}" ]]; then
  if [[ "${PEER_PORT}" == "19985" ]]; then
    OFFICIAL_NODE_MINER_API_PORT="10973"
  else
    OFFICIAL_NODE_MINER_API_PORT="$((PEER_PORT - 9000))"
  fi
else
  OFFICIAL_NODE_MINER_API_PORT="${OFFICIAL_NODE_MINER_API_PORT}"
fi
ANNOUNCEMENT_SEEDER_PID=""
OFFICIAL_NODE_ENABLE_WALLET="${OFFICIAL_NODE_ENABLE_WALLET:-0}"
OFFICIAL_NODE_TX_SEEDER_PID=""

case "${REQUIRE_M2_ANNOUNCEMENTS}" in
  1|strict)
    ANNOUNCEMENTS_CHECK=1
    ANNOUNCEMENTS_REQUIRED=1
    ANNOUNCEMENT_MODE="any"
    ;;
  all)
    ANNOUNCEMENTS_CHECK=1
    ANNOUNCEMENTS_REQUIRED=1
    ANNOUNCEMENT_MODE="all"
    ;;
  warn|optional)
    ANNOUNCEMENTS_CHECK=1
    ANNOUNCEMENTS_REQUIRED=0
    ANNOUNCEMENT_MODE="warn"
    ;;
  0|"")
    ;;
  *)
    echo "Invalid RUST_INTEROP_REQUIRE_M2_ANNOUNCEMENTS='${REQUIRE_M2_ANNOUNCEMENTS}'."
    echo "Use one of: 0 (default), 1|strict, all, or warn|optional."
    exit 1
    ;;
esac
OFFICIAL_CONFIG_FILE="${ROOT_DIR}/config/official-node/user.conf"
ACTUAL_RESULT="in_progress"
OPERATOR_NOTES=""
NETWORK_ID="unknown"
P2P_VERSION="unknown"
OFFICIAL_REPOSITORY="unknown"

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

EXPECTED_RESULT="required: handshake (Hello) and ping/pong exchange"
if [[ "${REQUIRE_M2}" -eq 1 ]]; then
  EXPECTED_RESULT="required: handshake + ping/pong + M2 request+response markers"
  if [[ "${ANNOUNCEMENTS_REQUIRED}" -eq 1 ]]; then
    if [[ "${ANNOUNCEMENT_MODE}" == "all" ]]; then
      EXPECTED_RESULT="required: handshake + ping/pong + M2 request+response + all announcement markers"
    else
      EXPECTED_RESULT="required: handshake + ping/pong + M2 request+response + at least one announcement marker"
    fi
  elif [[ "${ANNOUNCEMENTS_CHECK}" -eq 1 ]]; then
    EXPECTED_RESULT="required: handshake + ping/pong + M2 request+response (+ optional announcement markers)"
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

http_request() {
  local method=$1
  local url=$2
  local body="${3:-}"
  local code
  local tmp

  if command -v curl >/dev/null 2>&1; then
    tmp="$(mktemp)"
    if [[ "${body}" == "" ]]; then
      code="$(curl -sS -m 5 -o "${tmp}" -w '%{http_code}' -X "${method}" "${url}" || true)"
    else
      code="$(printf '%s' "${body}" | curl -sS -m 5 -H 'Content-Type: application/json' -o "${tmp}" -w '%{http_code}' -X "${method}" -d @- "${url}" || true)"
    fi
    if [[ "${code}" == "" ]]; then
      code="000"
    fi
    if [[ "${code}" =~ ^2 ]]; then
      cat "${tmp}"
      rm -f "${tmp}"
      return 0
    fi
    rm -f "${tmp}"
    return 1
  fi

  local ps_exe=""

  if command -v python3 >/dev/null 2>&1; then
    python3 - "$method" "$url" "$body" <<'PY'
import sys
from urllib.request import Request, urlopen

method, url = sys.argv[1], sys.argv[2]
body = sys.argv[3] if len(sys.argv) > 3 else ""
payload = body.encode() if body else None
headers = {"Content-Type": "application/json"} if payload is not None else {}
try:
    with urlopen(Request(url, data=payload, method=method, headers=headers), timeout=5) as response:
        code = response.getcode()
        if 200 <= code < 300:
            print(response.read().decode(), end="")
            raise SystemExit(0)
        raise SystemExit(1)
except Exception:
    raise SystemExit(1)
PY
    return
  fi

  if command -v powershell.exe >/dev/null 2>&1; then
    ps_exe="powershell.exe"
  elif [ -x "/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe" ]; then
    ps_exe="/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe"
  fi

  if [[ "${ps_exe}" != "" ]]; then
    if [[ "${body}" == "" ]]; then
      "${ps_exe}" -NoProfile -NonInteractive -Command "try { \$resp = Invoke-WebRequest -UseBasicParsing -Method '${method}' -Uri '${url}'; if (\$resp.StatusCode -ge 200 -and \$resp.StatusCode -lt 300) { if ('${method}' -eq 'GET') { Write-Output \$resp.Content }; exit 0 } else { exit 1 } } catch { exit 1 }"
    else
      "${ps_exe}" -NoProfile -NonInteractive -Command "try { \$resp = Invoke-WebRequest -UseBasicParsing -Method '${method}' -Uri '${url}' -ContentType 'application/json' -Body '${body}'; if (\$resp.StatusCode -ge 200 -and \$resp.StatusCode -lt 300) { Write-Output \$resp.Content; exit 0 } else { exit 1 } } catch { exit 1 }"
    fi
    return
  fi

  return 1
}

http_post() {
  local url=$1
  http_request POST "${url}" "${2:-}"
}

http_put() {
  local url=$1
  http_request PUT "${url}" "${2:-}"
}

http_get_json() {
  local url=$1
  http_request GET "${url}"
}

extract_json_string() {
  local json=$1
  local field=$2
  printf '%s' "${json}" | sed -n "s/.*\"${field}\":\"\\([^\"]*\\)\".*/\\1/p" | head -n 1
}

mine_official_block() {
  local from_group=$1
  local to_group=$2
  local query
  local mine_one_url

  query="fromGroup=${from_group}&toGroup=${to_group}"
  mine_one_url="http://${PEER_HOST}:${OFFICIAL_NODE_MINER_API_PORT}/miners/cpu-mining/mine-one-block?${query}"
  if http_post "${mine_one_url}"; then
    return 0
  fi

  mine_one_url="http://${PEER_HOST}:${OFFICIAL_NODE_REST_PORT}/miners/cpu-mining/mine-one-block?${query}"
  if http_post "${mine_one_url}"; then
    return 0
  fi

  return 1
}

start_announcement_seed_loop() {
  local miner_api_ready=0
  local rest_ready=0

  for _ in $(seq 1 "${OFFICIAL_READY_TIMEOUT}"); do
    if is_port_open "${PEER_HOST}" "${OFFICIAL_NODE_MINER_API_PORT}"; then
      miner_api_ready=1
      break
    fi
    if is_port_open "${PEER_HOST}" "${OFFICIAL_NODE_REST_PORT}"; then
      rest_ready=1
      break
    fi
    sleep 1
  done

  if [[ "${miner_api_ready}" -eq 0 ]] && [[ "${rest_ready}" -eq 0 ]]; then
    echo "announcement seeder skipped: official API/Miner API not reachable on ${PEER_HOST}:${OFFICIAL_NODE_MINER_API_PORT} or ${OFFICIAL_NODE_REST_PORT} after ${OFFICIAL_READY_TIMEOUT}s" >&2
    append_operator_note "announcement seeder failed to start: official API/Miner API not reachable."
    return 1
  fi

  echo "starting official block seeder for full announcement coverage (all mode)"
  (
    while true; do
      mine_official_block 0 0 >/dev/null 2>&1 || true
      mine_official_block 1 1 >/dev/null 2>&1 || true
      mine_official_block 2 2 >/dev/null 2>&1 || true
      mine_official_block 3 3 >/dev/null 2>&1 || true
      sleep 2
    done
  ) &
  ANNOUNCEMENT_SEEDER_PID=$!

  if [[ "${miner_api_ready}" -eq 1 ]]; then
    append_operator_note "announcement seeder started against ${PEER_HOST}:${OFFICIAL_NODE_MINER_API_PORT}/miners/cpu-mining/mine-one-block"
  elif [[ "${rest_ready}" -eq 1 ]]; then
    append_operator_note "announcement seeder started against ${PEER_HOST}:${OFFICIAL_NODE_REST_PORT}/miners/cpu-mining/mine-one-block"
  fi
}

start_tx_seed_loop() {
  local wallet_name="interop-tx-seed-$(date +%s)-$((RANDOM + RANDOM))"
  local wallet_password="interop-tx-seed-pass"
  local wallet_mnemonic='toward outdoor daughter deny mansion bench water alien crumble mother exchange screen salute antenna abuse key hair crisp debate goose great market core screen'
  local wallet_restore_url="http://${PEER_HOST}:${OFFICIAL_NODE_REST_PORT}/wallets"
  local wallet_unlock_url="http://${PEER_HOST}:${OFFICIAL_NODE_REST_PORT}/wallets/${wallet_name}/unlock"
  local wallet_addresses_url="http://${PEER_HOST}:${OFFICIAL_NODE_REST_PORT}/wallets/${wallet_name}/addresses"
  local wallet_transfer_url="http://${PEER_HOST}:${OFFICIAL_NODE_REST_PORT}/wallets/${wallet_name}/transfer"
  local wallet_addresses_json
  local wallet_active_addr
  local transfer_payload
  local restore_ok=0
  local unlock_ok=0
  local addresses_ok=0
  local transfer_ok=0
  local i
  local rest_ready=0

  for _ in $(seq 1 "${OFFICIAL_READY_TIMEOUT}"); do
    if is_port_open "${PEER_HOST}" "${OFFICIAL_NODE_REST_PORT}"; then
      rest_ready=1
      break
    fi
    sleep 1
  done

  if [[ "${rest_ready}" -eq 0 ]]; then
    append_operator_note "tx seeder failed: official REST API not reachable on ${PEER_HOST}:${OFFICIAL_NODE_REST_PORT} after ${OFFICIAL_READY_TIMEOUT}s."
    return 1
  fi

  for i in $(seq 1 12); do
    if http_put \
      "${wallet_restore_url}" \
      "{\"password\":\"${wallet_password}\",\"mnemonic\":\"${wallet_mnemonic}\",\"walletName\":\"${wallet_name}\"}"; then
      restore_ok=1
      break
    fi
    sleep 1
  done
  if [[ "${restore_ok}" -ne 1 ]]; then
    append_operator_note "tx seeder failed: cannot restore sender wallet."
    return 1
  fi

  for i in $(seq 1 12); do
    if http_post "${wallet_unlock_url}" "{\"password\":\"${wallet_password}\"}"; then
      unlock_ok=1
      break
    fi
    sleep 1
  done
  if [[ "${unlock_ok}" -ne 1 ]]; then
    append_operator_note "tx seeder failed: cannot unlock sender wallet."
    return 1
  fi

  for i in $(seq 1 12); do
    wallet_addresses_json="$(http_get_json "${wallet_addresses_url}")"
    if [[ "${wallet_addresses_json}" != "" ]]; then
      addresses_ok=1
      break
    fi
    sleep 1
  done
  if [[ "${addresses_ok}" -ne 1 ]]; then
    append_operator_note "tx seeder failed: cannot query sender wallet addresses."
    return 1
  fi

  wallet_active_addr="$(extract_json_string "${wallet_addresses_json}" "activeAddress")"
  if [[ "${wallet_active_addr}" == "" ]]; then
    append_operator_note "tx seeder failed: sender wallet active address not found."
    return 1
  fi

  transfer_payload="{\"destinations\":[{\"address\":\"${wallet_active_addr}\",\"attoAlphAmount\":\"1\",\"tokens\":[]}]}"
  for i in $(seq 1 12); do
    if http_post "${wallet_transfer_url}" "${transfer_payload}"; then
      transfer_ok=1
      break
    fi
    sleep 1
  done
  if [[ "${transfer_ok}" -ne 1 ]]; then
    append_operator_note "tx seeder failed: sender wallet transfer request failed."
    return 1
  fi

  echo "starting official wallet tx seeder for full announcement coverage (all mode)"
  (
    while true; do
      http_post "${wallet_transfer_url}" "${transfer_payload}" >/dev/null 2>&1 || true
      sleep 2
    done
  ) &
  OFFICIAL_NODE_TX_SEEDER_PID=$!
  append_operator_note "tx seeder started for wallet ${wallet_name} against ${wallet_restore_url}"
}

append_operator_note() {
  local note=$1
  if [[ "${OPERATOR_NOTES}" == "" ]]; then
    OPERATOR_NOTES="${note}"
  else
    OPERATOR_NOTES="${OPERATOR_NOTES}; ${note}"
  fi
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

  if [[ "${ANNOUNCEMENT_SEEDER_PID}" != "" ]]; then
    if kill -0 "${ANNOUNCEMENT_SEEDER_PID}" 2>/dev/null; then
      kill "${ANNOUNCEMENT_SEEDER_PID}" 2>/dev/null || true
    fi
  fi

  if [[ "${OFFICIAL_NODE_TX_SEEDER_PID}" != "" ]]; then
    if kill -0 "${OFFICIAL_NODE_TX_SEEDER_PID}" 2>/dev/null; then
      kill "${OFFICIAL_NODE_TX_SEEDER_PID}" 2>/dev/null || true
    fi
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
official_repository: ${OFFICIAL_REPOSITORY}
official_commit_or_release: ${official_commit}
official_build_command: ./scripts/official-node/build.sh
official_start_command: ${official_start_command}
official_listen_address: ${PEER_ADDRESS}
network_id: ${NETWORK_ID}
p2p_version: ${P2P_VERSION}
rust_commit: ${rust_commit}
rust_build_command: cargo build -p alephium-app
rust_start_command: cargo run -p alephium-app -- ${PEER_ADDRESS}
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

if [[ "${ATTACH_TO_EXISTING}" -eq 0 ]]; then
  if is_port_open "${PEER_HOST}" "${PEER_PORT}"; then
    echo "WARNING: ${PEER_ADDRESS} already has a listening process." >&2
    if [[ "${PEER_PORT}" -eq 19973 || "${PEER_PORT}" -eq 19974 || "${PEER_PORT}" -eq 19975 || "${PEER_PORT}" -eq 19985 ]]; then
      if [[ "${PEER_PORT}" -eq 19985 ]]; then
        echo "Peringatan: 19985 adalah port default repo ini. Java/Alephium lokal juga sering memakai port ini." >&2
        echo "Jika Java harus tetap jalan, jalankan dengan ATTACH mode atau ganti RUST_INTEROP_PEER_PORT." >&2
      fi
      echo "Peringatan: port ${PEER_PORT} sering dipakai node Java; sesuaikan dengan OFFICIAL_NODE_P2P_PORT/RUST_INTEROP_PEER_PORT jika bentrok." >&2
      append_operator_note "startup blocked: ${PEER_ADDRESS} already listened and is Java-common."
    fi
    append_operator_note "startup blocked: ${PEER_ADDRESS} already listening."
    echo "Jika Java node Anda sedang jalan di port ini, gunakan ATTACH mode agar tidak bentrok." >&2
    echo "Lanjutkan hanya dengan membunuh proses yang dipakai port itu atau pindah ke port lain dengan RUST_INTEROP_PEER_ADDRESS." >&2
    exit 1
  fi

  "${ROOT_DIR}/scripts/official-node/build.sh"
  case "${ANNOUNCEMENT_MODE}" in
    all)
      case "${OFFICIAL_NODE_AUTO_MINE_FOR_DEV}" in
        1|true|TRUE|True|yes|YES|Yes|on|ON|On)
          ;;
        *)
          OFFICIAL_NODE_AUTO_MINE_FOR_DEV=1
          append_operator_note "Enabled auto-mine-for-dev for full announcement mode."
          ;;
      esac
      case "${OFFICIAL_NODE_ENABLE_WALLET}" in
        1|true|TRUE|True|yes|YES|Yes|on|ON|On)
          ;;
        *)
          OFFICIAL_NODE_ENABLE_WALLET=1
          append_operator_note "Enabled wallet API for full announcement mode."
          ;;
      esac
      ;;
  esac
  OFFICIAL_NODE_RESET=1 \
  OFFICIAL_NODE_AUTO_MINE_FOR_DEV="${OFFICIAL_NODE_AUTO_MINE_FOR_DEV}" \
  OFFICIAL_NODE_ENABLE_WALLET="${OFFICIAL_NODE_ENABLE_WALLET}" \
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
  if [[ "${PEER_PORT}" -eq 19973 || "${PEER_PORT}" -eq 19974 || "${PEER_PORT}" -eq 19975 || "${PEER_PORT}" -eq 19985 ]]; then
    if [[ "${PEER_PORT}" -eq 19985 ]]; then
      echo "Peringatan: 19985 adalah port default repo ini. Pastikan node Java tidak ikut dimatikan saat ATTACH." >&2
    fi
    echo "Peringatan: port ${PEER_PORT} sering dipakai node Java. Jika bentrok, gunakan RUST_INTEROP_PEER_ADDRESS/PORT lain." >&2
    append_operator_note "ATTACH mode used with Java-common port ${PEER_PORT}."
  fi
  if ! is_port_open "${PEER_HOST}" "${PEER_PORT}"; then
    echo "no peer listening at ${PEER_ADDRESS}; start one externally or run without RUST_INTEROP_ATTACH_TO_EXISTING" >&2
    append_operator_note "ATTACH mode requested but peer not reachable at ${PEER_ADDRESS}."
    exit 1
  fi
  echo "attaching to existing peer at ${PEER_ADDRESS} (skip official node startup/cleanup)"
  echo "WARNING: ATTACH mode aktif, script tidak akan menyalakan atau mematikan node Java/official." >&2
  append_operator_note "ATTACH mode aktif; tidak memulai/mematikan proses official/JVM."
fi

if [[ "${ANNOUNCEMENT_MODE}" == "all" ]]; then
  if ! start_announcement_seed_loop; then
    echo "announcement seeder failed in all-announcement mode" >&2
    cat "${RUST_ERROR_FILE}" >&2
    exit 1
  fi

  if ! start_tx_seed_loop; then
    echo "tx seeder failed in all-announcement mode" >&2
    cat "${RUST_ERROR_FILE}" >&2
    exit 1
  fi
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
m2_chain_state_seen=0
m2_chain_state_fallback_headers_seen=0
m2_new_block_seen=0
m2_new_header_seen=0
m2_new_inv_seen=0
m2_new_txhashes_seen=0

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
    if grep -q "received ChainState" "${RUST_LOG_FILE}"; then
      m2_chain_state_seen=1
    fi
    if grep -q "sending fallback HeadersRequest with empty locator" "${RUST_LOG_FILE}"; then
      m2_chain_state_fallback_headers_seen=1
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
    if grep -q "received NewBlock" "${RUST_LOG_FILE}"; then
      m2_new_block_seen=1
    fi
    if grep -q "received NewHeader" "${RUST_LOG_FILE}"; then
      m2_new_header_seen=1
    fi
    if grep -q "received NewInv" "${RUST_LOG_FILE}"; then
      m2_new_inv_seen=1
    fi
    if grep -q "received NewTxHashes" "${RUST_LOG_FILE}"; then
      m2_new_txhashes_seen=1
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
  append_operator_note "basic handshake+ping assertions not observed before deadline."
  cat "${RUST_LOG_FILE}" >&2
  cat "${RUST_ERROR_FILE}" >&2
  exit 1
fi

if [[ "${REQUIRE_M2}" -eq 1 ]]; then
  if [[ "${m2_inv_request_seen}" -ne 1 || "${m2_headers_request_seen}" -ne 1 || "${m2_blocks_request_seen}" -ne 1 || "${m2_txs_request_seen}" -ne 1 ]]; then
    echo "M2 request markers not all observed before timeout" >&2
    append_operator_note "M2 request markers missing."
    cat "${RUST_LOG_FILE}" >&2
    cat "${RUST_ERROR_FILE}" >&2
    exit 1
  fi
  if [[ "${m2_chain_state_seen}" -ne 1 ]] && [[ "${m2_chain_state_fallback_headers_seen}" -ne 1 ]]; then
    echo "M2 chain-state marker not observed before timeout" >&2
    append_operator_note "M2 chain-state marker / fallback request not observed."
    cat "${RUST_LOG_FILE}" >&2
    cat "${RUST_ERROR_FILE}" >&2
    exit 1
  fi
  if [[ "${m2_headers_response_seen}" -ne 1 || "${m2_blocks_response_seen}" -ne 1 || "${m2_txs_response_seen}" -ne 1 ]]; then
    echo "M2 response markers not all observed before timeout" >&2
    append_operator_note "M2 response markers missing."
    cat "${RUST_LOG_FILE}" >&2
    cat "${RUST_ERROR_FILE}" >&2
    exit 1
  fi

  if [[ "${ANNOUNCEMENTS_CHECK}" -eq 1 ]]; then
    announcement_count=$((m2_new_block_seen + m2_new_header_seen + m2_new_inv_seen + m2_new_txhashes_seen))
    if [[ "${ANNOUNCEMENT_MODE}" == "all" ]]; then
      if [[ "${m2_new_block_seen}" -ne 1 || "${m2_new_header_seen}" -ne 1 || "${m2_new_inv_seen}" -ne 1 || "${m2_new_txhashes_seen}" -ne 1 ]]; then
        echo "M2 announcement markers not all observed before timeout" >&2
        append_operator_note "M2 announcement markers missing."
        cat "${RUST_LOG_FILE}" >&2
        cat "${RUST_ERROR_FILE}" >&2
        exit 1
      fi
    elif [[ "${announcement_count}" -eq 0 ]]; then
      if [[ "${ANNOUNCEMENTS_REQUIRED}" -eq 1 ]]; then
        echo "M2 announcement markers were not observed before timeout" >&2
        append_operator_note "No M2 announcement markers observed in strict mode."
        cat "${RUST_LOG_FILE}" >&2
        cat "${RUST_ERROR_FILE}" >&2
        exit 1
      fi
      append_operator_note "M2 announcement markers were requested in optional mode and were not observed."
      echo "M2 announcement markers were not all observed before timeout (optional mode)."
    fi
  fi
fi
