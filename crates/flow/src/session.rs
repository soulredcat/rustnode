use std::error::Error as StdError;
use std::fmt;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use alephium_conf::NodeConfig;
use alephium_crypto::{PrivateKey, PublicKey, sign};
use alephium_protocol::message::payload::{
    CHAIN_STATE_CODE, ChainState, CliqueId, HELLO_CODE, Hello, INV_REQUEST_CODE, INV_RESPONSE_CODE,
    InterBrokerInfo, InvRequest, InvResponse, NEW_BLOCK_CODE, NEW_BLOCK_HASH_CODE, NEW_HEADER_CODE,
    NEW_INV_CODE, NEW_TX_HASHES_CODE, NewTxHashes, PING_CODE, PONG_CODE, Ping, Pong, RequestId,
    TXS_REQUEST_CODE, TXS_RESPONSE_CODE, TxsRequest, TxsResponse, decode_chain_state_payload,
    decode_hello_payload, decode_inv_request_payload, decode_inv_response_payload,
    decode_new_txhashes_payload, decode_ping_payload, decode_pong_payload,
    decode_txs_request_payload, decode_txs_response_payload, encode_hello_payload,
    encode_inv_request_payload, encode_inv_response_payload, encode_ping_payload,
    encode_pong_payload, encode_txs_request_payload, encode_txs_response_payload,
};
use alephium_protocol::message::{
    FrameDecode, NetworkMagic, decode_frame_prefix, decode_raw_message_data, encode_frame,
    encode_raw_message_data,
};
use alephium_serde::Error as SerdeError;

use crate::event::BotEvent;
use crate::ipc::EventSink;
use crate::request_tracker::TxRequestTracker;

pub const STATUS: &str = "Alephium transaction-announcement fast path enabled";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionState {
    Connecting,
    HelloSent,
    Established,
}

#[derive(Debug)]
struct Session {
    state: SessionState,
    next_request_id: u32,
    outstanding_ping: Option<u32>,
    ping_deadline: Option<Instant>,
    next_ping_at: Instant,
    outstanding_inv_request: Option<u32>,
    tx_tracker: TxRequestTracker,
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

pub fn run(config: &NodeConfig, events: &EventSink) -> Result<(), FastPathError> {
    let result = run_session(config, events);
    if let Err(error) = &result {
        let _ = events.publish(BotEvent::peer_disconnected(
            &error.to_string(),
            unix_time_ns().unwrap_or_default(),
        ));
    }
    result
}

fn run_session(config: &NodeConfig, events: &EventSink) -> Result<(), FastPathError> {
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
            config.tx_request_batch_size,
            config.max_tracked_tx_ids,
            config.tx_request_timeout,
            config.max_tx_request_retries,
        ),
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
            events,
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
        }
    }
}

fn process_inbound(
    magic: &NetworkMagic,
    stream: &mut TcpStream,
    config: &NodeConfig,
    events: &EventSink,
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
            HELLO_CODE => handle_hello(stream, config, events, session, payload)?,
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
                handle_chain_state(state);
            }
            NEW_TX_HASHES_CODE => {
                let hashes = decode_new_txhashes_payload(payload)?.finish(payload_len)?;
                handle_new_tx_hashes(stream, config, events, session, hashes)?;
            }
            TXS_REQUEST_CODE => {
                let request = decode_txs_request_payload(payload)?.finish(payload_len)?;
                handle_txs_request(stream, config, request)?;
            }
            TXS_RESPONSE_CODE => {
                let response = decode_txs_response_payload(payload)?.finish(payload_len)?;
                handle_txs_response(events, session, response);
            }
            NEW_BLOCK_CODE | NEW_HEADER_CODE | NEW_INV_CODE | NEW_BLOCK_HASH_CODE => {}
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
    stream: &mut TcpStream,
    config: &NodeConfig,
    events: &EventSink,
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
    let _ = events.publish(BotEvent::peer_connected(
        &config.peer_address,
        unix_time_ns()?,
    ));
    send_readiness_probe(stream, config, session)
}

fn send_readiness_probe(
    stream: &mut TcpStream,
    config: &NodeConfig,
    session: &mut Session,
) -> Result<(), FastPathError> {
    if session.outstanding_inv_request.is_some() {
        return Ok(());
    }
    let request = InvRequest {
        id: RequestId {
            value: next_request_id(&mut session.next_request_id),
        },
        locators: vec![Vec::new()],
    };
    let payload = encode_inv_request_payload(&request)
        .map_err(|error| FastPathError::Protocol(format!("encode InvRequest failed: {error}")))?;
    send_payload(stream, config, INV_REQUEST_CODE, &payload)?;
    session.outstanding_inv_request = Some(request.id.value);
    println!("sent InvRequest id={} as readiness probe", request.id.value);
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
    println!("received ChainState with {} tip(s)", state.tips.len());
}

fn handle_new_tx_hashes(
    stream: &mut TcpStream,
    config: &NodeConfig,
    events: &EventSink,
    session: &mut Session,
    announcement: NewTxHashes,
) -> Result<(), FastPathError> {
    let observed_at = unix_time_ns()?;
    let mut accepted = 0usize;
    for (chain_index, transaction_ids) in announcement.hashes {
        for transaction_id in transaction_ids {
            if session.tx_tracker.enqueue(chain_index, transaction_id) {
                accepted = accepted.saturating_add(1);
                let _ = events.publish(BotEvent::transaction_announced(
                    chain_index.from,
                    chain_index.to,
                    transaction_id.bytes,
                    observed_at,
                ));
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

fn handle_txs_response(events: &EventSink, session: &mut Session, response: TxsResponse) {
    let response_id = response.id.value;
    let completion = session.tx_tracker.complete(response_id, response.txs);
    if completion.unknown_request {
        eprintln!("ignored unsolicited TxsResponse id={response_id}");
        return;
    }

    let observed_at = unix_time_ns().unwrap_or_default();
    for transaction in completion.transactions {
        let (chain_from, chain_to) = transaction
            .chain_index
            .map(|chain| (chain.from, chain.to))
            .unwrap_or((-1, -1));
        let _ = events.publish(BotEvent::raw_transaction(
            chain_from,
            chain_to,
            transaction.transaction_id,
            transaction.raw_transaction,
            observed_at,
        ));
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

fn unix_time_ns() -> Result<u64, FastPathError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| FastPathError::Protocol(format!("invalid system time: {error}")))?;
    u64::try_from(duration.as_nanos())
        .map_err(|error| FastPathError::Protocol(format!("nanosecond timestamp overflow: {error}")))
}
