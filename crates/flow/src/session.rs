use std::error::Error as StdError;
use std::fmt;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::PoisonError;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use alephium_conf::NodeConfig;
use alephium_crypto::{PrivateKey, PublicKey, sign};
use alephium_protocol::message::payload::{
    BLOCKS_AND_UNCLES_BY_HEIGHTS_REQUEST_CODE, BLOCKS_AND_UNCLES_BY_HEIGHTS_RESPONSE_CODE,
    BLOCKS_REQUEST_CODE, BLOCKS_RESPONSE_CODE,
};
use alephium_protocol::message::payload::{
    CHAIN_STATE_CODE, ChainState, CliqueId, HELLO_CODE, Hello, INV_REQUEST_CODE, INV_RESPONSE_CODE,
    InterBrokerInfo, InvRequest, InvResponse, NEW_BLOCK_CODE, NEW_BLOCK_HASH_CODE, NEW_HEADER_CODE,
    NEW_INV_CODE, NEW_TX_HASHES_CODE, NewTxHashes, PING_CODE, PONG_CODE, Ping, Pong, RequestId,
    TXS_REQUEST_CODE, TXS_RESPONSE_CODE, TxsRequest, TxsResponse, decode_chain_state_payload,
    decode_hello_payload, decode_inv_request_payload, decode_inv_response_payload,
    decode_new_txhashes_payload, decode_ping_payload, decode_pong_payload,
    decode_txs_request_payload, encode_hello_payload, encode_inv_response_payload,
    encode_ping_payload, encode_pong_payload, encode_txs_request_payload,
    encode_txs_response_payload,
};
use alephium_protocol::message::sync::{
    decode_blocks_and_uncles_by_heights_response, decode_blocks_response, decode_new_block,
    decode_new_block_hash, encode_blocks_and_uncles_by_heights_request, encode_blocks_request,
    encode_chain_state,
};
use alephium_protocol::message::{
    FrameDecode, NetworkMagic, decode_frame_prefix, decode_raw_message_data,
    decode_single_txs_response_payload, encode_frame, encode_raw_message_data,
};
use alephium_serde::Error as SerdeError;

use crate::mempool::{RawMempoolTx, SharedRawMempool};
use crate::request_tracker::TxRequestTracker;
use crate::sync::{BlockSync, SharedChainStore};

pub const STATUS: &str = "Alephium P2P transaction acquisition enabled";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionState {
    Connecting,
    HelloSent,
    Established,
}

struct Session {
    state: SessionState,
    next_request_id: u32,
    outstanding_ping: Option<u32>,
    ping_deadline: Option<Instant>,
    next_ping_at: Instant,
    outstanding_inv_request: Option<u32>,
    tx_tracker: TxRequestTracker,
    block_sync: Option<BlockSync>,
    next_sync_log: Instant,
    /// Last `ChainState` sent, resent when it changes or every 10 s.
    sent_chain_state: Option<(Vec<u8>, Instant)>,
}

#[derive(Debug)]
pub enum FastPathError {
    Io(io::Error),
    Serde(SerdeError),
    Protocol(String),
}

impl fmt::Display for FastPathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Serde(error) => write!(formatter, "codec error: {error}"),
            Self::Protocol(message) => write!(formatter, "protocol error: {message}"),
        }
    }
}

impl StdError for FastPathError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Serde(error) => Some(error),
            Self::Protocol(_) => None,
        }
    }
}

impl From<io::Error> for FastPathError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<SerdeError> for FastPathError {
    fn from(error: SerdeError) -> Self {
        Self::Serde(error)
    }
}

/// Runs one P2P session. With `chain`, blocks are followed into it.
pub fn run(
    config: &NodeConfig,
    mempool: &SharedRawMempool,
    chain: Option<&SharedChainStore>,
) -> Result<(), FastPathError> {
    let mut stream = TcpStream::connect(&config.peer_address)?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(config.io_poll_interval))?;
    stream.set_write_timeout(Some(config.tx_request_timeout))?;

    let mut session = Session {
        state: SessionState::Connecting,
        next_request_id: 1,
        outstanding_ping: None,
        ping_deadline: None,
        next_ping_at: Instant::now() + config.ping_interval,
        outstanding_inv_request: None,
        tx_tracker: TxRequestTracker::new(
            config.max_inflight_tx_requests,
            config.max_tracked_tx_ids,
            config.tx_request_timeout,
            config.max_tx_request_retries,
        ),
        block_sync: chain.map(|chain| {
            BlockSync::new(chain.clone(), config.configured_groups, config.network_id)
        }),
        next_sync_log: Instant::now(),
        sent_chain_state: None,
    };

    send_hello(&mut stream, config)?;
    session.state = SessionState::HelloSent;

    let mut inbound = Vec::with_capacity(64 * 1024);
    let mut read_buffer = vec![0u8; 64 * 1024];
    let magic = NetworkMagic::new(config.network_magic);

    loop {
        match stream.read(&mut read_buffer) {
            Ok(0) => {
                return Err(FastPathError::Protocol(format!(
                    "remote closed while in {:?} state",
                    session.state
                )));
            }
            Ok(bytes_read) => {
                let new_len = inbound.len().saturating_add(bytes_read);
                let max_inbound = config.max_frame_data.saturating_mul(2);
                if new_len > max_inbound {
                    return Err(FastPathError::Protocol(format!(
                        "inbound buffer exceeded {max_inbound} bytes"
                    )));
                }
                inbound.extend_from_slice(&read_buffer[..bytes_read]);
            }
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.kind() == io::ErrorKind::TimedOut => {}
            Err(error) => return Err(error.into()),
        }

        process_inbound(
            &magic,
            &mut stream,
            config,
            mempool,
            &mut inbound,
            &mut session,
        )?;

        if session.state == SessionState::Established {
            let now = Instant::now();
            if let Some(deadline) = session.ping_deadline
                && now >= deadline
            {
                return Err(FastPathError::Protocol(format!(
                    "ping timeout waiting for request {}",
                    session.outstanding_ping.unwrap_or_default()
                )));
            }

            if now >= session.next_ping_at {
                send_ping(&mut stream, config, &mut session)?;
            }

            let (_requeued, exhausted) = session.tx_tracker.expire(now);
            if exhausted > 0 {
                eprintln!("fastpath exhausted retries for {exhausted} transaction(s)");
            }
            flush_tx_requests(&mut stream, config, &mut session, now)?;
            request_blocks(&mut stream, config, &mut session, now)?;
        }
    }
}

fn request_blocks(
    stream: &mut TcpStream,
    config: &NodeConfig,
    session: &mut Session,
    now: Instant,
) -> Result<(), FastPathError> {
    let Some(sync) = session.block_sync.as_mut() else {
        return Ok(());
    };
    if let Some(request) = sync.next_request(now, &mut session.next_request_id) {
        let payload = encode_blocks_and_uncles_by_heights_request(request.id, &request.ranges);
        send_payload(
            stream,
            config,
            BLOCKS_AND_UNCLES_BY_HEIGHTS_REQUEST_CODE,
            &payload,
        )?;
        let blocks: i32 = request
            .ranges
            .iter()
            .map(|range| range.to_height - range.from_height + 1)
            .sum();
        println!(
            "sent BlocksAndUnclesByHeightsRequest id={} chains={} heights={blocks}",
            request.id,
            request.ranges.len()
        );
    }
    // v2 peers decide whether they are synced from our ChainState; without it
    // they never consider themselves synced and stop relaying blocks.
    if let Some(tips) = sync.chain_state() {
        let payload = encode_chain_state(&tips);
        let due = session.sent_chain_state.as_ref().is_none_or(|(last, at)| {
            *last != payload || now.duration_since(*at) >= std::time::Duration::from_secs(10)
        });
        if due {
            send_payload(stream, config, CHAIN_STATE_CODE, &payload)?;
            session.sent_chain_state = Some((payload, now));
        }
    }
    if now >= session.next_sync_log {
        session.next_sync_log = now + std::time::Duration::from_secs(10);
        println!(
            "blocks followed={} orphans={} weight checks={} mismatches={}",
            sync.added,
            sync.orphan_count(),
            sync.weight_checks,
            sync.weight_mismatches
        );
    }
    Ok(())
}

fn process_inbound(
    magic: &NetworkMagic,
    stream: &mut TcpStream,
    config: &NodeConfig,
    mempool: &SharedRawMempool,
    inbound: &mut Vec<u8>,
    session: &mut Session,
) -> Result<(), FastPathError> {
    loop {
        if inbound.is_empty() {
            return Ok(());
        }

        let frame = match decode_frame_prefix(inbound, *magic, config.max_frame_data)? {
            FrameDecode::NeedMoreData { required_total } => {
                if required_total > config.max_frame_data.saturating_add(64) {
                    return Err(FastPathError::Protocol(format!(
                        "peer requested oversized frame: {required_total} bytes"
                    )));
                }
                return Ok(());
            }
            FrameDecode::Complete(frame) => frame,
        };

        let raw = decode_raw_message_data(frame.data)?;
        let payload = raw.payload_body;
        let payload_len = payload.len();
        match raw.payload_code {
            HELLO_CODE => handle_hello(config, session, payload)?,
            PING_CODE => {
                let ping = decode_ping_payload(payload)?.finish(payload_len)?;
                handle_ping(stream, config, session, ping)?;
            }
            PONG_CODE => {
                let pong = decode_pong_payload(payload)?.finish(payload_len)?;
                handle_pong(session, pong)?;
            }
            INV_REQUEST_CODE => {
                let request = decode_inv_request_payload(payload)?.finish(payload_len)?;
                handle_inv_request(stream, config, request)?;
            }
            INV_RESPONSE_CODE => {
                let response = decode_inv_response_payload(payload)?.finish(payload_len)?;
                handle_inv_response(session, response);
            }
            CHAIN_STATE_CODE => {
                let state = decode_chain_state_payload(payload)?.finish(payload_len)?;
                if let Some(sync) = session.block_sync.as_mut() {
                    sync.on_chain_state(&state.tips);
                }
                handle_chain_state(state);
            }
            BLOCKS_AND_UNCLES_BY_HEIGHTS_RESPONSE_CODE => {
                if let Some(sync) = session.block_sync.as_mut() {
                    let (id, blocks) = decode_blocks_and_uncles_by_heights_response(payload)?;
                    let received: usize = blocks.iter().map(Vec::len).sum();
                    let added = sync.on_blocks(id, blocks);
                    println!(
                        "received BlocksAndUnclesByHeightsResponse id={id} blocks={received} added={}",
                        added.len()
                    );
                }
            }
            NEW_BLOCK_CODE => {
                if let Some(sync) = session.block_sync.as_mut() {
                    let block = decode_new_block(payload)?;
                    for added in sync.on_new_blocks(vec![block]) {
                        println!(
                            "followed block {} {}->{} height={} canonical={}",
                            added.hash.to_hex(),
                            added.from,
                            added.to,
                            added.height,
                            added.canonical
                        );
                    }
                }
            }
            NEW_TX_HASHES_CODE => {
                let hashes = decode_new_txhashes_payload(payload)?.finish(payload_len)?;
                handle_new_tx_hashes(stream, config, session, hashes)?;
            }
            TXS_REQUEST_CODE => {
                let request = decode_txs_request_payload(payload)?.finish(payload_len)?;
                handle_txs_request(stream, config, request)?;
            }
            TXS_RESPONSE_CODE => {
                let response = decode_single_txs_response_payload(payload)?.finish(payload_len)?;
                handle_txs_response(mempool, session, response);
            }
            NEW_BLOCK_HASH_CODE => {
                let hash = decode_new_block_hash(payload)?;
                if let Some(sync) = session.block_sync.as_mut()
                    && sync.should_fetch(&hash)
                {
                    let id = next_request_id(&mut session.next_request_id);
                    let payload = encode_blocks_request(id, &[hash]);
                    send_payload(stream, config, BLOCKS_REQUEST_CODE, &payload)?;
                }
            }
            BLOCKS_RESPONSE_CODE => {
                if let Some(sync) = session.block_sync.as_mut() {
                    let (_, blocks) = decode_blocks_response(payload)?;
                    for added in sync.on_new_blocks(blocks) {
                        println!(
                            "followed block {} {}->{} height={} canonical={}",
                            added.hash.to_hex(),
                            added.from,
                            added.to,
                            added.height,
                            added.canonical
                        );
                    }
                }
            }
            NEW_HEADER_CODE | NEW_INV_CODE => {}
            unsupported if session.state == SessionState::Established => {
                eprintln!("fastpath ignored unsupported payload code {unsupported}");
            }
            unsupported => {
                return Err(FastPathError::Protocol(format!(
                    "unsupported payload code {unsupported} before handshake"
                )));
            }
        }

        inbound.drain(..frame.consumed);
    }
}

fn handle_hello(
    config: &NodeConfig,
    session: &mut Session,
    payload: &[u8],
) -> Result<(), FastPathError> {
    if session.state != SessionState::HelloSent {
        return Err(FastPathError::Protocol(format!(
            "unexpected Hello in {:?} state",
            session.state
        )));
    }

    let hello = decode_hello_payload(payload, config.configured_groups)?;
    println!(
        "received valid Hello from {} ({}/{})",
        hello.client_id, hello.broker_info.broker_num, hello.broker_info.broker_id
    );
    session.state = SessionState::Established;
    Ok(())
}

fn handle_inv_request(
    stream: &mut TcpStream,
    config: &NodeConfig,
    request: InvRequest,
) -> Result<(), FastPathError> {
    let response = InvResponse {
        id: request.id,
        hashes: vec![Vec::new(); request.locators.len()],
    };
    let payload = encode_inv_response_payload(&response)
        .map_err(|error| FastPathError::Protocol(format!("encode InvResponse failed: {error}")))?;
    send_payload(stream, config, INV_RESPONSE_CODE, &payload)
}

fn handle_inv_response(session: &mut Session, response: InvResponse) {
    if session.outstanding_inv_request == Some(response.id.value) {
        session.outstanding_inv_request = None;
    }
}

fn handle_chain_state(state: ChainState) {
    let heights: Vec<i32> = state.tips.iter().map(|tip| tip.height).collect();
    println!("received ChainState heights={heights:?}");
}

fn handle_new_tx_hashes(
    stream: &mut TcpStream,
    config: &NodeConfig,
    session: &mut Session,
    announcement: NewTxHashes,
) -> Result<(), FastPathError> {
    let mut accepted = 0usize;
    for (chain_index, transaction_ids) in announcement.hashes {
        for transaction_id in transaction_ids {
            if session.tx_tracker.enqueue(chain_index, transaction_id) {
                accepted = accepted.saturating_add(1);
            }
        }
    }

    if accepted > 0 {
        println!("received NewTxHashes accepted={accepted}");
        flush_tx_requests(stream, config, session, Instant::now())?;
    }
    Ok(())
}

fn flush_tx_requests(
    stream: &mut TcpStream,
    config: &NodeConfig,
    session: &mut Session,
    now: Instant,
) -> Result<(), FastPathError> {
    for request in session
        .tx_tracker
        .build_ready_requests(&mut session.next_request_id, now)
    {
        let payload = encode_txs_request_payload(&request).map_err(|error| {
            FastPathError::Protocol(format!("encode TxsRequest failed: {error}"))
        })?;
        send_payload(stream, config, TXS_REQUEST_CODE, &payload)?;
        println!(
            "sent TxsRequest id={} chains={} txs={}",
            request.id.value,
            request.hashes.len(),
            request
                .hashes
                .iter()
                .map(|(_, transactions)| transactions.len())
                .sum::<usize>()
        );
    }
    Ok(())
}

fn handle_txs_response(mempool: &SharedRawMempool, session: &mut Session, response: TxsResponse) {
    let response_id = response.id.value;
    let completion = session.tx_tracker.complete(response_id, response.txs);
    if completion.unknown_request {
        eprintln!("ignored unsolicited TxsResponse id={response_id}");
        return;
    }
    if completion.malformed_response {
        eprintln!("rejected malformed TxsResponse id={response_id}");
        return;
    }

    if !completion.transactions.is_empty() {
        let seen_at_unix_ms = now_millis().unwrap_or_default();
        let mut pool = mempool.lock().unwrap_or_else(PoisonError::into_inner);
        for transaction in completion.transactions {
            pool.insert(
                transaction.transaction_id,
                RawMempoolTx {
                    chain_from: transaction.chain_index.from,
                    chain_to: transaction.chain_index.to,
                    raw: transaction.raw_transaction,
                    seen_at_unix_ms,
                },
            );
        }
    }

    if completion.requeued > 0 || completion.exhausted > 0 {
        eprintln!(
            "TxsResponse id={response_id} requeued={} exhausted={}",
            completion.requeued, completion.exhausted
        );
    }
}

fn handle_txs_request(
    stream: &mut TcpStream,
    config: &NodeConfig,
    request: TxsRequest,
) -> Result<(), FastPathError> {
    let response = TxsResponse {
        id: request.id,
        txs: Vec::new(),
    };
    let payload = encode_txs_response_payload(&response)
        .map_err(|error| FastPathError::Protocol(format!("encode TxsResponse failed: {error}")))?;
    send_payload(stream, config, TXS_RESPONSE_CODE, &payload)
}

fn handle_ping(
    stream: &mut TcpStream,
    config: &NodeConfig,
    session: &Session,
    ping: Ping,
) -> Result<(), FastPathError> {
    if session.state != SessionState::Established {
        return Err(FastPathError::Protocol("Ping before handshake".to_owned()));
    }
    let payload = encode_pong_payload(&Pong { id: ping.id });
    send_payload(stream, config, PONG_CODE, &payload)
}

fn handle_pong(session: &mut Session, pong: Pong) -> Result<(), FastPathError> {
    let Some(expected) = session.outstanding_ping else {
        return Err(FastPathError::Protocol(format!(
            "unexpected Pong id={}",
            pong.id.value
        )));
    };
    if expected != pong.id.value {
        return Err(FastPathError::Protocol(format!(
            "Pong mismatch: expected {expected}, got {}",
            pong.id.value
        )));
    }
    session.outstanding_ping = None;
    session.ping_deadline = None;
    println!("received matching pong {}", pong.id.value);
    Ok(())
}

fn send_hello(stream: &mut TcpStream, config: &NodeConfig) -> Result<(), FastPathError> {
    let private_key = PrivateKey::try_from(&config.private_key[..])
        .map_err(|error| FastPathError::Protocol(format!("invalid private key: {error}")))?;
    let clique_id = PublicKey::try_from(&config.clique_id[..])
        .map_err(|error| FastPathError::Protocol(format!("invalid clique id: {error}")))?;
    let broker_info = InterBrokerInfo {
        clique_id: CliqueId {
            bytes: clique_id.bytes(),
        },
        broker_id: config.broker_id,
        broker_num: config.broker_num,
    };
    let signature = sign(&broker_info.hash(), &private_key)
        .map_err(|error| FastPathError::Protocol(format!("cannot sign Hello: {error}")))?;
    let hello = Hello {
        client_id: config.client_id.clone(),
        timestamp: now_millis()?,
        broker_info,
        signature: signature.bytes(),
    };
    let payload = encode_hello_payload(&hello)
        .map_err(|error| FastPathError::Protocol(format!("encode Hello failed: {error}")))?;
    send_payload(stream, config, HELLO_CODE, &payload)?;
    println!(
        "sent Hello: client={} broker_id={} broker_num={}",
        hello.client_id, hello.broker_info.broker_id, hello.broker_info.broker_num
    );
    Ok(())
}

fn send_ping(
    stream: &mut TcpStream,
    config: &NodeConfig,
    session: &mut Session,
) -> Result<(), FastPathError> {
    if session.outstanding_ping.is_some() {
        return Ok(());
    }
    let request_id = next_request_id(&mut session.next_request_id);
    let payload = encode_ping_payload(&Ping {
        id: RequestId { value: request_id },
        timestamp: now_millis()?,
    });
    send_payload(stream, config, PING_CODE, &payload)?;
    session.outstanding_ping = Some(request_id);
    session.ping_deadline = Some(Instant::now() + config.ping_timeout);
    session.next_ping_at = Instant::now() + config.ping_interval;
    println!("sent ping {request_id}");
    Ok(())
}

fn send_payload(
    stream: &mut TcpStream,
    config: &NodeConfig,
    payload_code: i32,
    payload_body: &[u8],
) -> Result<(), FastPathError> {
    let raw_message = encode_raw_message_data(payload_code, payload_body)?;
    let frame = encode_frame(
        NetworkMagic::new(config.network_magic),
        &raw_message,
        config.max_frame_data,
    )?;
    stream.write_all(&frame)?;
    stream.flush()?;
    Ok(())
}

fn next_request_id(next_request_id: &mut u32) -> u32 {
    let value = *next_request_id;
    *next_request_id = (*next_request_id).wrapping_add(1);
    if *next_request_id == 0 {
        *next_request_id = 1;
    }
    value
}

fn now_millis() -> Result<i64, FastPathError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| FastPathError::Protocol(format!("invalid system time: {error}")))?;
    i64::try_from(duration.as_millis())
        .map_err(|error| FastPathError::Protocol(format!("timestamp overflow: {error}")))
}
