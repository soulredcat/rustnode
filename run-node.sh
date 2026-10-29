#!/usr/bin/env bash
# Runs rustnode next to the operator's official mainnet node.
#
#   ./run-node.sh          run in this terminal (Ctrl+C stops it)
#   ./run-node.sh --bg     run in the background, log to $RUSTNODE_DATA/rustnode.log
#   ./run-node.sh stop     stop the background node
#   ./run-node.sh status   show whether the background node runs
#
# Settings (environment, all optional):
#   RUSTNODE_DATA        data folder        (default ~/.local/rustnode)
#   RUSTNODE_API_BIND    API address        (default 127.0.0.1:22973)
#   RUSTNODE_UPSTREAM    official REST API  (default http://127.0.0.1:12973)
#   RUSTNODE_PEER        official P2P peer  (default 127.0.0.1:9973)
#   RUSTNODE_WINDOW      heights per chain  (default 100000)
#   RUSTNODE_STATE_FROM  official node `db` folder: copy the world state from it
#   RUSTNODE_API_KEY     the official node's api-key, if it has one
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
data="${RUSTNODE_DATA:-$HOME/.local/rustnode}"
bin="$root/target/release/rustnode"
pid_file="$data/rustnode.pid"
log_file="$data/rustnode.log"

running_pid() {
    if [[ -f "$pid_file" ]]; then
        local pid
        pid="$(cat "$pid_file")"
        if kill -0 "$pid" 2>/dev/null; then
            echo "$pid"
            return
        fi
    fi
    echo ""
}

case "${1:-}" in
    stop)
        pid="$(running_pid)"
        if [[ -z "$pid" ]]; then
            echo "rustnode is not running"
            exit 0
        fi
        kill -TERM "$pid"
        echo "stopping rustnode (pid $pid)"
        for _ in $(seq 1 30); do
            kill -0 "$pid" 2>/dev/null || { rm -f "$pid_file"; echo "stopped"; exit 0; }
            sleep 1
        done
        echo "rustnode did not stop within 30 s; pid $pid" >&2
        exit 1
        ;;
    status)
        pid="$(running_pid)"
        if [[ -n "$pid" ]]; then
            echo "rustnode is running (pid $pid), log: $log_file"
        else
            echo "rustnode is not running"
        fi
        exit 0
        ;;
    --bg | "") ;;
    *)
        echo "usage: $0 [--bg | stop | status]" >&2
        exit 2
        ;;
esac

if [[ -n "$(running_pid)" ]]; then
    echo "rustnode is already running (pid $(running_pid))"
    exit 0
fi

if [[ ! -x "$bin" ]]; then
    echo "building rustnode (the first build takes a few minutes)"
    (cd "$root" && cargo build --release -p alephium-app)
fi

mkdir -p "$data"
args=(
    --api-bind "${RUSTNODE_API_BIND:-127.0.0.1:22973}"
    --upstream "${RUSTNODE_UPSTREAM:-http://127.0.0.1:12973}"
    --peer "${RUSTNODE_PEER:-127.0.0.1:9973}"
    --data "$data/chain.redb"
    --state "$data/state.redb"
    --watch
)
[[ -n "${RUSTNODE_WINDOW:-}" ]] && args+=(--window "$RUSTNODE_WINDOW")
[[ -n "${RUSTNODE_STATE_FROM:-}" ]] && args+=(--state-from "$RUSTNODE_STATE_FROM")

export RUSTNODE_IDENTITY_PATH="$data/node-identity.bin"

if [[ "${1:-}" == "--bg" ]]; then
    nohup "$bin" "${args[@]}" >>"$log_file" 2>&1 &
    echo $! >"$pid_file"
    echo "rustnode started (pid $!), log: $log_file"
else
    exec "$bin" "${args[@]}"
fi
