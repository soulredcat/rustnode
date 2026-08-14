# Integrating `alphrustalbitrage` with the Rust Fast-Path

## 1. Integration rule

The Rust event stream is an additional speculative trigger. It does not replace the current official-node transaction and block sources.

Use a dual-source model:

```text
Rust RawTransaction
    -> speculative decode / overlay / affected-route quote

Official node accepted transaction
    -> validate correlation
    -> allow execution pipeline to continue
```

## 2. Start the fast-path process

For a local official node P2P listener:

```bash
cargo run --release -p alephium-app -- \
  --peer 127.0.0.1:9973 \
  --ipc-bind 127.0.0.1:19090 \
  --max-inflight 8 \
  --tx-batch-size 1 \
  --request-timeout-ms 250 \
  --event-queue-capacity 4096 \
  --watch
```

Keep the official node REST/WebSocket services enabled because the bot still needs them for canonical state and execution.

## 3. Bot configuration

Add explicit configuration to `alphrustalbitrage`:

```dotenv
BOT_FASTPATH_ENABLED=false
BOT_FASTPATH_ENDPOINT=127.0.0.1:19090
BOT_FASTPATH_CONNECT_TIMEOUT_MS=1000
BOT_FASTPATH_READ_TIMEOUT_MS=1000
BOT_FASTPATH_MAX_FRAME_BYTES=2097152
BOT_FASTPATH_REQUIRE_OFFICIAL_ACCEPTANCE=true
```

Initial deployment must run with `BOT_FASTPATH_ENABLED=false`, then shadow mode, before enabling speculative preparation.

## 4. Recommended bot modules

```text
crates/engine/src/infrastructure/fastpath/
├── mod.rs
├── client.rs          TCP reconnect and bounded reader
├── decoder.rs         ALFP v1 frame decoder
├── event.rs           typed event DTOs
└── sequence_guard.rs  restart/gap detection

crates/engine/src/runtime/live/trigger/
└── fastpath_source.rs converts RawTransaction into existing trigger observations
```

Do not put route selection, quote math, wallet assignment, or execution policy in the infrastructure client.

## 5. Rust event decoder foundation

```rust
use std::io::{self, Read};
use std::net::TcpStream;

const HEADER_LEN: usize = 68;
const MAGIC: &[u8; 4] = b"ALFP";
const VERSION: u16 = 1;
const MAX_PAYLOAD: usize = 2 * 1024 * 1024;

#[derive(Debug)]
pub struct FastPathEvent {
    pub kind: u8,
    pub flags: u8,
    pub sequence: u64,
    pub observed_at_unix_ns: u64,
    pub chain_from: i32,
    pub chain_to: i32,
    pub transaction_id: [u8; 32],
    pub payload: Vec<u8>,
}

pub fn read_event(stream: &mut TcpStream) -> io::Result<FastPathEvent> {
    let mut header = [0u8; HEADER_LEN];
    stream.read_exact(&mut header)?;
    if &header[0..4] != MAGIC {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad ALFP magic"));
    }

    let version = u16::from_be_bytes([header[4], header[5]]);
    if version != VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported ALFP version {version}"),
        ));
    }

    let payload_len = u32::from_be_bytes(header[64..68].try_into().unwrap()) as usize;
    if payload_len > MAX_PAYLOAD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "ALFP payload exceeds configured limit",
        ));
    }

    let mut transaction_id = [0u8; 32];
    transaction_id.copy_from_slice(&header[32..64]);
    let mut payload = vec![0u8; payload_len];
    stream.read_exact(&mut payload)?;

    Ok(FastPathEvent {
        kind: header[6],
        flags: header[7],
        sequence: u64::from_be_bytes(header[8..16].try_into().unwrap()),
        observed_at_unix_ns: u64::from_be_bytes(header[16..24].try_into().unwrap()),
        chain_from: i32::from_be_bytes(header[24..28].try_into().unwrap()),
        chain_to: i32::from_be_bytes(header[28..32].try_into().unwrap()),
        transaction_id,
        payload,
    })
}
```

The production implementation should replace `unwrap()` with typed errors and preserve the existing engine's bounded reconnect/recovery conventions.

## 6. Mapping events into the bot

### `TransactionAnnounced` (`kind = 2`)

Use only for metrics and fetch-race correlation. Do not scan routes yet because raw transaction bytes are absent.

### `RawTransaction` (`kind = 3`)

1. Decode the canonical `TransactionTemplate`, recompute `unsigned.id`, and require it to equal `transaction_id`; hashing the complete template bytes is incorrect.
2. Decode using the bot's native transaction/script decoder.
3. Produce the existing `LiveMempoolTransaction` or a dedicated speculative DTO.
4. Identify known affected pools and tokens.
5. Apply a speculative virtual reserve overlay.
6. Run affected-route discovery and sizing from memory.
7. Store the prepared opportunity under `transaction_id` with a strict TTL.
8. Do not build/sign/submit yet.

### Official accepted event

When the existing official-node source reports the same transaction ID:

1. mark the speculative transaction accepted;
2. compare decoded intent and affected pools;
3. refresh or invalidate stale overlays;
4. perform the existing final quote, risk, pending, and wallet-capacity gates;
5. continue to build/preflight only if all official gates pass.

## 7. Sequence and recovery

Maintain:

```rust
struct SequenceGuard {
    last_sequence: Option<u64>,
}
```

For every event:

```text
first event              -> establish baseline
sequence == last + 1     -> normal
sequence <= last         -> duplicate/restart; reset speculative state
sequence > last + 1      -> gap; invalidate speculative state and recover from official source
TCP reconnect            -> treat as a new stream and reset speculative state
```

Never continue using a virtual overlay across an unexplained gap.

## 8. Rollout sequence

1. **Metrics-only:** connect, decode, record latency; no route scan.
2. **Shadow decode:** compare Rust raw decode against official-node transaction decode.
3. **Shadow opportunity:** calculate opportunities but never dispatch them.
4. **Speculative preparation:** precompute routes; official event still unlocks execution.
5. **Measured production:** enable only after p99 and correctness gates pass.

## 9. Required race metrics

Add monotonic timestamps for:

```text
p2p_announcement_received
p2p_tx_request_sent
p2p_raw_transaction_received
fastpath_event_queued
fastpath_event_received_by_bot
official_ws_transaction_received
affected_pools_identified
opportunity_selected
submit_started
mempool_observed
```

Primary comparison:

```text
fastpath_lead_ms = official_ws_transaction_received - fastpath_event_received_by_bot
```

Do not claim the fast path is useful until p50, p95, and p99 demonstrate a repeatable lead without materially increasing missed or inconsistent transactions.
