#!/usr/bin/env bash
# Differential check of rustnode against the operator's synced official
# mainnet node, the only network rustnode serves (IMMUTABLE.md).
#
#   OFFICIAL_URL   official REST API          (default http://127.0.0.1:12973)
#   OFFICIAL_PEER  official P2P address       (default 127.0.0.1:9973)
#   API_BIND       rustnode gateway address   (default 127.0.0.1:22973)
#   WINDOW         heights kept per chain     (default 300)
#   KEEP_WORK=1    keep the store and logs instead of deleting them
#
# It checks, in order:
#   1. the official node serves mainnet and is synced;
#   2. raw block encoding and block JSON of the newest 100 heights per chain;
#   3. a fresh rustnode bootstraps WINDOW heights into its store, follows the
#      chain over P2P (Hello, Ping/Pong, ChainState weights equal to the
#      peer's), serves blocks in shadow mode with no mismatch, and its gateway
#      answers like the official node;
#   4. after being killed, rustnode reopens the store and fetches only the
#      heights it missed.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OFFICIAL_URL="${OFFICIAL_URL:-http://127.0.0.1:12973}"
OFFICIAL_PEER="${OFFICIAL_PEER:-127.0.0.1:9973}"
API_BIND="${API_BIND:-127.0.0.1:22973}"
WINDOW="${WINDOW:-300}"
VIA="http://${API_BIND}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/rustnode-mainnet-check.XXXXXX")"
RUSTNODE_PID=""

cleanup() {
  if [[ -n "${RUSTNODE_PID}" ]] && kill -0 "${RUSTNODE_PID}" 2>/dev/null; then
    kill -TERM "${RUSTNODE_PID}" 2>/dev/null || true
    wait "${RUSTNODE_PID}" 2>/dev/null || true
  fi
  if [[ "${KEEP_WORK:-0}" == "1" ]]; then
    echo "kept: ${WORK}"
  else
    rm -rf "${WORK}"
  fi
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

json() {
  python3 -I -c "import json, sys; data = json.load(sys.stdin); print($1)"
}

official() {
  curl -fsS -m 10 "${OFFICIAL_URL}$1"
}

echo "== official node"
official /infos/chain-params | json 'data["networkId"]' | grep -qx 0 ||
  fail "${OFFICIAL_URL} is not a mainnet node"
official /infos/self-clique | json 'data["synced"]' | grep -qx True ||
  fail "${OFFICIAL_URL} is not synced"

echo "== block encoding and JSON, newest 100 heights per chain"
(
  cd "${ROOT_DIR}"
  export RUSTNODE_OFFICIAL_URL="${OFFICIAL_URL}"
  cargo test -q -p alephium-model --test official_raw_blocks -- --ignored
  cargo test -q -p alephium-api --test official_block_json -- --ignored
)

echo "== rustnode build"
(cd "${ROOT_DIR}" && cargo build -q --release -p alephium-app)
BIN="${ROOT_DIR}/target/release/rustnode"

start_rustnode() {
  RUSTNODE_IDENTITY_PATH="${WORK}/node-identity.bin" "${BIN}" \
    --api-bind "${API_BIND}" --upstream "${OFFICIAL_URL}" \
    --peer "${OFFICIAL_PEER}" --window "${WINDOW}" \
    --data "${WORK}/chain.redb" --watch >"$1" 2>&1 &
  RUSTNODE_PID=$!
}

stop_rustnode() {
  kill -TERM "${RUSTNODE_PID}"
  wait "${RUSTNODE_PID}" 2>/dev/null || true
  RUSTNODE_PID=""
}

# wait_for <log> <pattern> <seconds>
wait_for() {
  local deadline=$((SECONDS + $3))
  until grep -qE "$2" "$1"; do
    kill -0 "${RUSTNODE_PID}" 2>/dev/null || { cat "$1" >&2; fail "rustnode exited"; }
    ((SECONDS < deadline)) || { tail -40 "$1" >&2; fail "no '$2' within $3 s"; }
    sleep 1
  done
}

# The newest following line must show blocks followed and no weight mismatch.
check_following() {
  local line
  line="$(grep -E 'blocks followed=' "$1" | tail -1)"
  [[ "${line}" =~ mismatches=0$ ]] || fail "weight mismatch: ${line}"
  [[ ! "${line}" =~ followed=0[[:space:]] ]] || fail "no block followed: ${line}"
  echo "   ${line}"
}

echo "== run 1: bootstrap ${WINDOW} heights per chain, follow, serve"
start_rustnode "${WORK}/run1.log"
wait_for "${WORK}/run1.log" 'bootstrap: imported' 600
grep -E 'bootstrap: (fetching|imported)' "${WORK}/run1.log" | sed 's/^/   /'
wait_for "${WORK}/run1.log" 'received valid Hello' 60
wait_for "${WORK}/run1.log" 'received matching pong' 60
wait_for "${WORK}/run1.log" 'seeded weights' 60
wait_for "${WORK}/run1.log" 'followed block|BlocksAndUnclesByHeightsResponse' 120
wait_for "${WORK}/run1.log" 'weight checks=[1-9]' 120

# Intra-group blocks across the window, through the gateway in shadow mode.
requested=0
for group in 0 1 2 3; do
  height="$(official "/blockflow/chain-info?fromGroup=${group}&toGroup=${group}" | json 'data["currentHeight"]')"
  for back in 3 50 150 $((WINDOW - 20)); do
    hash="$(official "/blockflow/hashes?fromGroup=${group}&toGroup=${group}&height=$((height - back))" |
      json 'data["headers"][0]')"
    status="$(curl -s -o /dev/null -w '%{http_code}' -m 10 "${VIA}/blockflow/blocks/${hash}")"
    [[ "${status}" == "200" ]] || fail "GET /blockflow/blocks/${hash} through rustnode: ${status}"
    requested=$((requested + 1))
  done
done

# Gateway: official build info, refusal outside the surface, and native
# address groups equal to the official answers. Addresses: official
# AddressSpec vectors, outputs of the newest mainnet blocks, a contract, a
# grouped and an invalid address.
curl -fsS -m 10 "${VIA}/infos/node" | grep -q releaseVersion || fail "GET /infos/node through rustnode"
[[ "$(curl -s -o /dev/null -w '%{http_code}' -m 10 "${VIA}/infos/self-clique")" == "404" ]] ||
  fail "GET /infos/self-clique is outside the surface and must be 404"
addresses=(
  1C2RAVWSuaXw8xtUxqVERR7ChKBE1XgscNFw73NSHE1v3
  1H7CmpbvGJwgyLzR91wzSJJSkiBC92WDPTWny4gmhQJQc
  1DkrQMni2h8KYpvY8t7dECshL66gwnxiR5uD2Udxps6og
  2jjvDdgGjC6X9HHMCMHohVfvp1uf3LHQrAGWaufR17P7AFwtxodTxSktqKc2urNEtaoUCy5xXpBUwpZ8QM8Q3e5BYCx
  je9CrJD444xMSGDA2yr1XMvugoHuTc6pfYEaPYrKLuYa
  rvpeCy7GhsGHq8n6TnB1LjQh4xn1FMHJVXnsdZAniKZA
  uqa8vVAySDGA5nbt6c5WXYXXuBWPghtDx6C8aE3VFXDr
  1C2RAVWSuaXw8xtUxqVERR7ChKBE1XgscNFw73NSHE1v3:1
  not-an-address
)
for group in 0 1 2 3; do
  height="$(official "/blockflow/chain-info?fromGroup=${group}&toGroup=${group}" | json 'data["currentHeight"]')"
  hash="$(official "/blockflow/hashes?fromGroup=${group}&toGroup=${group}&height=${height}" | json 'data["headers"][0]')"
  while read -r address; do
    [[ -n "${address}" ]] && addresses+=("${address}")
  done < <(official "/blockflow/blocks/${hash}" |
    json '"\n".join(sorted({o["address"] for t in data["transactions"] for o in t["unsigned"]["fixedOutputs"] + t["generatedOutputs"]})[:3])')
done
for address in "${addresses[@]}"; do
  direct="$(curl -s -m 10 -w ' %{http_code}' "${OFFICIAL_URL}/addresses/${address}/group" | tr -d ' \n\r\t')"
  via="$(curl -s -m 10 -w ' %{http_code}' "${VIA}/addresses/${address}/group" | tr -d ' \n\r\t')"
  [[ "${direct}" == "${via}" ]] || fail "address group of ${address}: official '${direct}', rustnode '${via}'"
done
echo "   gateway: ${#addresses[@]} address groups equal to the official node"

# The gateway logs shadow counts every 60 s.
wait_for "${WORK}/run1.log" "blockflow/blocks/\{hash\}: requests=${requested} " 90
shadow="$(grep -E 'blockflow/blocks/\{hash\}: requests=' "${WORK}/run1.log" | tail -1)"
[[ "${shadow}" =~ shadow_match=${requested}\ shadow_mismatch=0 ]] || fail "shadow check: ${shadow}"
echo "   shadow: ${requested} of ${requested} blocks equal to the official node"
check_following "${WORK}/run1.log"
rss_kib="$(ps -o rss= -p "${RUSTNODE_PID}" | tr -d ' ')"
echo "   rss $((rss_kib / 1024)) MiB, store $(du -h "${WORK}/chain.redb" | cut -f1)"

echo "== run 2: killed, restarted 30 s later, resumes from the store"
stop_rustnode
sleep 30
start_rustnode "${WORK}/run2.log"
wait_for "${WORK}/run2.log" 'bootstrap: continuing from the stored chain' 300
fetched="$(grep -oE 'bootstrap: fetching [0-9]+' "${WORK}/run2.log" | grep -oE '[0-9]+$' || echo 0)"
((fetched < 16 * WINDOW)) || fail "the restart fetched ${fetched} heights, a full bootstrap"
echo "   fetched ${fetched} missed heights instead of $((16 * WINDOW))"
wait_for "${WORK}/run2.log" 'followed block|BlocksAndUnclesByHeightsResponse' 120
wait_for "${WORK}/run2.log" 'weight checks=[1-9]' 120
check_following "${WORK}/run2.log"
stop_rustnode

echo "mainnet check passed"
