#![forbid(unsafe_code)]
//! Alephium networking, synchronization, validation, and BlockFlow ownership.
//!
//! This phase includes M1 bootstrap plus M2 message dispatch and decoding
//! with a minimal sync probe (initial InvRequest, optional HeadersRequest).

use std::error::Error as StdError;
use std::fmt;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use alephium_conf::NodeConfig;
use alephium_crypto::{sign, PrivateKey, PublicKey};
use alephium_protocol::message::{
    decode_frame_prefix, decode_raw_message_data, encode_frame, encode_raw_message_data, FrameDecode,
    NetworkMagic,
};
use alephium_protocol::message::payload::{
    decode_blocks_and_uncles_by_heights_request_payload,
    decode_blocks_and_uncles_by_heights_response_payload, decode_blocks_request_payload,
    decode_blocks_response_payload, decode_chain_state_payload, decode_headers_by_heights_request_payload,
    decode_headers_by_heights_response_payload, decode_headers_request_payload,
    decode_headers_response_payload, decode_hello_payload, decode_inv_request_payload,
    decode_inv_response_payload, decode_new_block_hash_payload, decode_new_block_payload,
    decode_new_header_payload, decode_new_inv_payload, decode_new_txhashes_payload, decode_ping_payload,
    decode_pong_payload, decode_txs_request_payload, decode_txs_response_payload,
    encode_blocks_request_payload, encode_headers_request_payload, encode_hello_payload,
    encode_inv_request_payload, encode_ping_payload, encode_pong_payload, encode_txs_request_payload,
    BLOCKS_AND_UNCLES_BY_HEIGHTS_REQUEST_CODE, BLOCKS_AND_UNCLES_BY_HEIGHTS_RESPONSE_CODE,
    BLOCKS_REQUEST_CODE, BLOCKS_RESPONSE_CODE, CHAIN_STATE_CODE, HELLO_CODE,
    HEADERS_BY_HEIGHTS_REQUEST_CODE, HEADERS_BY_HEIGHTS_RESPONSE_CODE, HEADERS_REQUEST_CODE,
    HEADERS_RESPONSE_CODE, INV_REQUEST_CODE, INV_RESPONSE_CODE, NEW_BLOCK_CODE, NEW_BLOCK_HASH_CODE,
    NEW_HEADER_CODE, NEW_INV_CODE, NEW_TX_HASHES_CODE, PING_CODE, PONG_CODE, TXS_REQUEST_CODE,
    TXS_RESPONSE_CODE, BlockHash,
    BlocksAndUnclesByHeightsRequest, BlocksAndUnclesByHeightsResponse, BlocksRequest, BlocksResponse,
    ChainIndex, ChainState, CliqueId, HeadersByHeightsRequest, HeadersByHeightsResponse,
    HeadersRequest, HeadersResponse, Hello, InterBrokerInfo, InvRequest, InvResponse, NewBlock,
    NewBlockHash, NewHeader, NewInv, NewTxHashes, Ping, Pong, RequestId, TxsRequest, TxsResponse,
    TransactionId,
};
use alephium_serde::Error as SerdeError;

/// Current implementation status.
pub const STATUS: &str =
    "M1 bootstrap complete; M2 request/response decoding and sync probe enabled in flow.";

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
    outstanding_headers_request: Option<u32>,
    outstanding_blocks_request: Option<u32>,
    outstanding_txs_request: Option<u32>,
    chain_state_tips: Vec<BlockHash>,
    sent_inv_request: bool,
    sent_headers_request: bool,
    sent_blocks_request: bool,
    sent_txs_request: bool,
}

#[derive(Debug)]
pub enum FlowError {
    Io(io::Error),
    Serde(SerdeError),
    Protocol(String),
}

impl fmt::Display for FlowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "io error: {error}"),
            Self::Serde(error) => write!(formatter, "serde error: {error}"),
            Self::Protocol(message) => write!(formatter, "protocol error: {message}"),
        }
    }
}

impl StdError for FlowError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Serde(error) => Some(error),
            Self::Protocol(_) => None,
        }
    }
}

impl From<io::Error> for FlowError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<SerdeError> for FlowError {
    fn from(error: SerdeError) -> Self {
        Self::Serde(error)
    }
}

/// Start an outbound M1/M2 bootstrap session and keep it alive until remote close or local failure.
pub fn run(config: &NodeConfig) -> Result<(), FlowError> {
    let mut stream = TcpStream::connect(&config.peer_address)?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_millis(250)))?;

    let mut session = Session {
        state: SessionState::Connecting,
        next_request_id: 1,
        outstanding_ping: None,
        ping_deadline: None,
        next_ping_at: Instant::now() + config.ping_interval,
        outstanding_inv_request: None,
        outstanding_headers_request: None,
        outstanding_blocks_request: None,
        outstanding_txs_request: None,
        chain_state_tips: Vec::new(),
        sent_inv_request: false,
        sent_headers_request: false,
        sent_blocks_request: false,
        sent_txs_request: false,
    };

    send_hello(&mut stream, config)?;
    session.state = SessionState::HelloSent;

    let mut inbound = Vec::new();
    let mut read_buffer = vec![0_u8; 4096];
    let magic = NetworkMagic::new(config.network_magic);

    loop {
        match stream.read(&mut read_buffer) {
            Ok(0) => {
                return Err(FlowError::Protocol(format!(
                    "remote closed while in {:?} state",
                    session.state
                )));
            }
            Ok(bytes) => inbound.extend_from_slice(&read_buffer[..bytes]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock
                || error.kind() == io::ErrorKind::TimedOut => {}
            Err(error) => return Err(error.into()),
        }

        process_inbound(&magic, &mut stream, config, &mut inbound, &mut session)?;

        if session.state == SessionState::Established {
            let now = Instant::now();
            if let Some(deadline) = session.ping_deadline && now >= deadline {
                return Err(FlowError::Protocol(format!(
                    "ping timeout after {}s waiting for request {}",
                    config.ping_timeout.as_secs(),
                    session.outstanding_ping.unwrap_or_default()
                )));
            }

            if now >= session.next_ping_at {
                send_ping(&mut stream, config, &mut session)?;
            }
        }
    }
}

fn process_inbound(
    magic: &NetworkMagic,
    stream: &mut TcpStream,
    config: &NodeConfig,
    inbound: &mut Vec<u8>,
    session: &mut Session,
) -> Result<(), FlowError> {
    loop {
        if inbound.is_empty() {
            return Ok(());
        }

        let frame = match decode_frame_prefix(inbound, *magic, config.max_frame_data)? {
            FrameDecode::NeedMoreData { required_total } => {
                if inbound.len() < required_total {
                    return Ok(());
                }
                continue;
            }
            FrameDecode::Complete(frame) => frame,
        };

        let raw = decode_raw_message_data(frame.data)?;
        let payload = raw.payload_body;
        let payload_len = payload.len();

        match raw.payload_code {
            HELLO_CODE => handle_hello(stream, config, session, payload)?,
            PING_CODE => {
                let ping = decode_ping_payload(payload)?.finish(payload_len)?;
                handle_ping(stream, config, session, ping)?;
            }
            PONG_CODE => {
                let pong = decode_pong_payload(payload)?.finish(payload_len)?;
                handle_pong(session, pong)?;
            }
            CHAIN_STATE_CODE => {
                let chain_state = decode_chain_state_payload(payload)?.finish(payload_len)?;
                handle_chain_state(stream, config, session, chain_state)?;
            }
            BLOCKS_REQUEST_CODE => {
                let request = decode_blocks_request_payload(payload)?.finish(payload_len)?;
                handle_blocks_request(request);
            }
            BLOCKS_RESPONSE_CODE => {
                let response = decode_blocks_response_payload(payload)?.finish(payload_len)?;
                handle_blocks_response(stream, config, session, response)?;
            }
            HEADERS_REQUEST_CODE => {
                let request = decode_headers_request_payload(payload)?.finish(payload_len)?;
                handle_headers_request(request);
            }
            HEADERS_RESPONSE_CODE => {
                let response = decode_headers_response_payload(payload)?.finish(payload_len)?;
                handle_headers_response(stream, config, session, response)?;
            }
            INV_REQUEST_CODE => {
                let request = decode_inv_request_payload(payload)?.finish(payload_len)?;
                handle_inv_request(request);
            }
            INV_RESPONSE_CODE => {
                let response = decode_inv_response_payload(payload)?.finish(payload_len)?;
                handle_inv_response(stream, config, session, response)?;
            }
            NEW_BLOCK_CODE => {
                let block = decode_new_block_payload(payload)?.finish(payload_len)?;
                handle_new_block(block);
            }
            NEW_HEADER_CODE => {
                let header = decode_new_header_payload(payload)?.finish(payload_len)?;
                handle_new_header(header);
            }
            NEW_INV_CODE => {
                let new_inv = decode_new_inv_payload(payload)?.finish(payload_len)?;
                handle_new_inv(new_inv);
            }
            NEW_BLOCK_HASH_CODE => {
                let block_hash = decode_new_block_hash_payload(payload)?.finish(payload_len)?;
                handle_new_block_hash(block_hash);
            }
            NEW_TX_HASHES_CODE => {
                let hashes = decode_new_txhashes_payload(payload)?.finish(payload_len)?;
                handle_new_txhashes(hashes);
            }
            TXS_REQUEST_CODE => {
                let request = decode_txs_request_payload(payload)?.finish(payload_len)?;
                handle_txs_request(request);
            }
            TXS_RESPONSE_CODE => {
                let response = decode_txs_response_payload(payload)?.finish(payload_len)?;
                handle_txs_response(session, response);
            }
            HEADERS_BY_HEIGHTS_REQUEST_CODE => {
                let request = decode_headers_by_heights_request_payload(payload)?.finish(payload_len)?;
                handle_headers_by_heights_request(request);
            }
            HEADERS_BY_HEIGHTS_RESPONSE_CODE => {
                let response =
                    decode_headers_by_heights_response_payload(payload)?.finish(payload_len)?;
                handle_headers_by_heights_response(response);
            }
            BLOCKS_AND_UNCLES_BY_HEIGHTS_REQUEST_CODE => {
                let request =
                    decode_blocks_and_uncles_by_heights_request_payload(payload)?.finish(payload_len)?;
                handle_blocks_and_uncles_by_heights_request(request);
            }
            BLOCKS_AND_UNCLES_BY_HEIGHTS_RESPONSE_CODE => {
                let response =
                    decode_blocks_and_uncles_by_heights_response_payload(payload)?.finish(payload_len)?;
                handle_blocks_and_uncles_by_heights_response(response);
            }
            unsupported => match session.state {
                SessionState::Established => {
                    println!("received unsupported payload code {unsupported} in Established state");
                    Ok(())
                }
                _ => Err(FlowError::Protocol(format!(
                    "unsupported payload code {unsupported}"
                ))),
            }?,
        }

        inbound.drain(0..frame.consumed);
    }
}

fn handle_hello(
    stream: &mut TcpStream,
    config: &NodeConfig,
    session: &mut Session,
    payload: &[u8],
) -> Result<(), FlowError> {
    if session.state != SessionState::HelloSent {
        return Err(FlowError::Protocol(format!(
            "unexpected hello in {:?} state",
            session.state
        )));
    }

    let hello = decode_hello_payload(payload, config.configured_groups)?;
    println!(
        "received valid Hello from {} ({}/{})",
        hello.client_id, hello.broker_info.broker_num, hello.broker_info.broker_id
    );
    session.state = SessionState::Established;
    send_inv_request(stream, config, session)?;
    Ok(())
}

fn send_inv_request(
    stream: &mut TcpStream,
    config: &NodeConfig,
    session: &mut Session,
) -> Result<(), FlowError> {
    if session.sent_inv_request {
        return Ok(());
    }

    let request = InvRequest {
        id: RequestId {
            value: next_request_id(&mut session.next_request_id),
        },
        locators: vec![Vec::new()],
    };
    let payload = encode_inv_request_payload(&request).map_err(|error| {
        FlowError::Protocol(format!("failed to encode initial InvRequest payload: {error}"))
    })?;
    send_payload(stream, config, INV_REQUEST_CODE, &payload)?;
    session.outstanding_inv_request = Some(request.id.value);
    session.sent_inv_request = true;
    println!("sent InvRequest id={} as initial sync probe", request.id.value);
    Ok(())
}

fn maybe_send_headers_probe(
    stream: &mut TcpStream,
    config: &NodeConfig,
    session: &mut Session,
    response: &InvResponse,
) -> Result<(), FlowError> {
    if session.sent_headers_request {
        return Ok(());
    }

    let Some(locator) = response
        .hashes
        .iter()
        .find_map(|chain| chain.first().copied())
        .or_else(|| session.chain_state_tips.first().copied())
    else {
        return Ok(());
    };

    send_headers_request(stream, config, session, locator)
}

fn send_headers_request(
    stream: &mut TcpStream,
    config: &NodeConfig,
    session: &mut Session,
    locator: BlockHash,
) -> Result<(), FlowError> {
    let request = HeadersRequest {
        id: RequestId {
            value: next_request_id(&mut session.next_request_id),
        },
        locators: vec![locator],
    };
    let payload = encode_headers_request_payload(&request).map_err(|error| {
        FlowError::Protocol(format!("failed to encode probe HeadersRequest: {error}"))
    })?;
    send_payload(stream, config, HEADERS_REQUEST_CODE, &payload)?;
    session.outstanding_headers_request = Some(request.id.value);
    session.sent_headers_request = true;
    println!("sent HeadersRequest id={} with 1 locator", request.id.value);
    Ok(())
}

fn handle_chain_state(
    stream: &mut TcpStream,
    config: &NodeConfig,
    session: &mut Session,
    chain_state: ChainState,
) -> Result<(), FlowError> {
    println!("received ChainState with {} tip(s)", chain_state.tips.len());
    session.chain_state_tips = chain_state
        .tips
        .into_iter()
        .map(|tip| tip.hash)
        .collect();

    if !session.sent_headers_request {
        if let Some(locator) = session.chain_state_tips.first().copied() {
            send_headers_request(stream, config, session, locator)?;
        }
    }

    Ok(())
}

fn handle_blocks_request(request: BlocksRequest) {
    println!(
        "received BlocksRequest id={} locators={}",
        request.id.value,
        request.locators.len()
    );
}

fn handle_blocks_response(
    stream: &mut TcpStream,
    config: &NodeConfig,
    session: &mut Session,
    response: BlocksResponse,
) -> Result<(), FlowError> {
    let expected = session.outstanding_blocks_request;
    if expected.is_none() {
        println!(
            "received unsolicited BlocksResponse id={}",
            response.id.value
        );
        return Ok(());
    }
    if expected != Some(response.id.value) {
        println!(
            "received BlocksResponse id={} expected={}",
            response.id.value,
            expected.unwrap_or_default()
        );
        return Ok(());
    }
    session.outstanding_blocks_request = None;

    println!(
        "received BlocksResponse id={} blocks={}",
        response.id.value,
        response.blocks.len()
    );
    maybe_send_txs_request(stream, config, session)?;
    Ok(())
}

fn handle_headers_request(request: HeadersRequest) {
    println!(
        "received HeadersRequest id={} locators={}",
        request.id.value,
        request.locators.len()
    );
}

fn handle_headers_response(
    stream: &mut TcpStream,
    config: &NodeConfig,
    session: &mut Session,
    response: HeadersResponse,
) -> Result<(), FlowError> {
    let expected = session.outstanding_headers_request;
    if expected.is_none() {
        println!(
            "received unsolicited HeadersResponse id={}",
            response.id.value
        );
        return Ok(());
    }
    if expected != Some(response.id.value) {
        println!(
            "received HeadersResponse id={} expected={}",
            response.id.value,
            expected.unwrap_or_default()
        );
        return Ok(());
    }
    session.outstanding_headers_request = None;

    println!(
        "received HeadersResponse id={} headers={}",
        response.id.value,
        response.headers.len()
    );
    maybe_send_blocks_request(stream, config, session, &response)?;
    Ok(())
}

fn maybe_send_blocks_request(
    stream: &mut TcpStream,
    config: &NodeConfig,
    session: &mut Session,
    response: &HeadersResponse,
) -> Result<(), FlowError> {
    if session.sent_blocks_request {
        return Ok(());
    }

    if response.headers.is_empty() {
        return Ok(());
    }

    let request = BlocksRequest {
        id: RequestId {
            value: next_request_id(&mut session.next_request_id),
        },
        locators: vec![BlockHash {
            bytes: [0_u8; 32],
        }],
    };
    let payload = encode_blocks_request_payload(&request).map_err(|error| {
        FlowError::Protocol(format!("failed to encode probe BlocksRequest: {error}"))
    })?;
    send_payload(stream, config, BLOCKS_REQUEST_CODE, &payload)?;
    session.outstanding_blocks_request = Some(request.id.value);
    session.sent_blocks_request = true;
    println!(
        "sent BlocksRequest id={} with {} locator(s)",
        request.id.value,
        request.locators.len()
    );
    Ok(())
}

fn maybe_send_txs_request(
    stream: &mut TcpStream,
    config: &NodeConfig,
    session: &mut Session,
) -> Result<(), FlowError> {
    if session.sent_txs_request {
        return Ok(());
    }

    let request = TxsRequest {
        id: RequestId {
            value: next_request_id(&mut session.next_request_id),
        },
        hashes: vec![(
            ChainIndex { from: 0, to: 0 },
            vec![TransactionId {
                bytes: [0x02_u8; 32],
            }],
        )],
    };
    let payload = encode_txs_request_payload(&request).map_err(|error| {
        FlowError::Protocol(format!("failed to encode probe TxsRequest: {error}"))
    })?;
    send_payload(stream, config, TXS_REQUEST_CODE, &payload)?;
    session.outstanding_txs_request = Some(request.id.value);
    session.sent_txs_request = true;
    println!(
        "sent TxsRequest id={} chains={} txs={}",
        request.id.value,
        request.hashes.len(),
        request.hashes.iter().map(|(_, txs)| txs.len()).sum::<usize>()
    );
    Ok(())
}

fn handle_inv_request(request: InvRequest) {
    let count: usize = request.locators.iter().map(|chain| chain.len()).sum();
    println!(
        "received InvRequest id={} chains={} hashes={}",
        request.id.value,
        request.locators.len(),
        count
    );
}

fn handle_inv_response(
    stream: &mut TcpStream,
    config: &NodeConfig,
    session: &mut Session,
    response: InvResponse,
) -> Result<(), FlowError> {
    let count: usize = response.hashes.iter().map(|chain| chain.len()).sum();
    let request_id = response.id.value;
    let expected_id = session.outstanding_inv_request;
    if expected_id == Some(request_id) {
        session.outstanding_inv_request = None;
        println!(
            "received InvResponse id={} chains={} hashes={} (matched)",
            request_id,
            response.hashes.len(),
            count
        );
    } else {
        println!(
            "received InvResponse id={} chains={} hashes={} (unsolicited)",
            request_id,
            response.hashes.len(),
            count
        );
    }

    maybe_send_headers_probe(stream, config, session, &response)
}

fn handle_new_block(block: NewBlock) {
    println!("received NewBlock with {} bytes", block.bytes.len());
}

fn handle_new_header(header: NewHeader) {
    println!("received NewHeader with {} bytes", header.bytes.len());
}

fn handle_new_inv(new_inv: NewInv) {
    let total: usize = new_inv.hashes.iter().map(|chain| chain.len()).sum();
    println!(
        "received NewInv with {} chain(s), {} hashes",
        new_inv.hashes.len(),
        total
    );
}

fn handle_new_block_hash(block_hash: NewBlockHash) {
    println!("received NewBlockHash bytes {}", block_hash.hash.bytes.len());
}

fn handle_new_txhashes(new_tx_hashes: NewTxHashes) {
    let chain_count = new_tx_hashes.hashes.len();
    let tx_count: usize = new_tx_hashes
        .hashes
        .iter()
        .map(|(_, txs)| txs.len())
        .sum();
    println!("received NewTxHashes chains={} txs={}", chain_count, tx_count);
}

fn handle_txs_request(request: TxsRequest) {
    let chain_count = request.hashes.len();
    let tx_count: usize = request.hashes.iter().map(|(_, txs)| txs.len()).sum();
    println!(
        "received TxsRequest id={} chains={} txs={}",
        request.id.value,
        chain_count,
        tx_count
    );
}

fn handle_txs_response(session: &mut Session, response: TxsResponse) {
    let expected = session.outstanding_txs_request;
    if expected.is_none() {
        println!("received unsolicited TxsResponse id={}", response.id.value);
        return;
    }
    if expected != Some(response.id.value) {
        println!(
            "received TxsResponse id={} expected={}",
            response.id.value,
            expected.unwrap_or_default()
        );
        return;
    }
    session.outstanding_txs_request = None;

    println!(
        "received TxsResponse id={} txs={}",
        response.id.value,
        response.txs.len()
    );
}

fn handle_headers_by_heights_request(request: HeadersByHeightsRequest) {
    println!(
        "received HeadersByHeightsRequest id={} chains={}",
        request.id.value,
        request.data.len()
    );
}

fn handle_headers_by_heights_response(response: HeadersByHeightsResponse) {
    println!(
        "received HeadersByHeightsResponse id={} headers={}",
        response.id.value,
        response.headers.len()
    );
}

fn handle_blocks_and_uncles_by_heights_request(request: BlocksAndUnclesByHeightsRequest) {
    println!(
        "received BlocksAndUnclesByHeightsRequest id={} chains={}",
        request.id.value,
        request.data.len()
    );
}

fn handle_blocks_and_uncles_by_heights_response(response: BlocksAndUnclesByHeightsResponse) {
    println!(
        "received BlocksAndUnclesByHeightsResponse id={} blocks={}",
        response.id.value,
        response.blocks.len()
    );
}

fn handle_ping(
    stream: &mut TcpStream,
    config: &NodeConfig,
    session: &mut Session,
    ping: Ping,
) -> Result<(), FlowError> {
    if session.state != SessionState::Established {
        return Err(FlowError::Protocol(format!(
            "unexpected ping in {:?} state",
            session.state
        )));
    }

    println!("received ping id={}", ping.id.value);
    let pong = encode_pong_payload(&Pong { id: ping.id });
    send_payload(stream, config, PONG_CODE, &pong)?;
    Ok(())
}

fn handle_pong(session: &mut Session, pong: Pong) -> Result<(), FlowError> {
    if session.state != SessionState::Established {
        return Err(FlowError::Protocol(format!(
            "unexpected pong in {:?} state",
            session.state
        )));
    }

    let Some(expected) = session.outstanding_ping else {
        return Err(FlowError::Protocol(format!("unexpected pong {}", pong.id.value)));
    };
    if expected != pong.id.value {
        return Err(FlowError::Protocol(format!(
            "pong mismatch: expected {expected}, got {}",
            pong.id.value
        )));
    }

    session.outstanding_ping = None;
    session.ping_deadline = None;
    println!("received matching pong {}", pong.id.value);
    Ok(())
}

fn send_hello(stream: &mut TcpStream, config: &NodeConfig) -> Result<(), FlowError> {
    let now = now_millis()?;
    let private_key = PrivateKey::try_from(&config.private_key[..])
        .map_err(|error| FlowError::Protocol(format!("invalid private key: {error}")))?;
    let clique_id = PublicKey::try_from(&config.clique_id[..])
        .map_err(|error| FlowError::Protocol(format!("invalid clique id: {error}")))?;

    let broker_info = InterBrokerInfo {
        clique_id: CliqueId {
            bytes: clique_id.bytes(),
        },
        broker_id: config.broker_id,
        broker_num: config.broker_num,
    };
    let signature = sign(&broker_info.hash(), &private_key).map_err(|error| {
        FlowError::Protocol(format!("cannot sign hello payload for local identity: {error}"))
    })?;

    let hello = Hello {
        client_id: config.client_id.clone(),
        timestamp: now,
        broker_info,
        signature: signature.bytes(),
    };
    let payload = encode_hello_payload(&hello)
        .map_err(|error| FlowError::Protocol(format!("encode local hello failed: {error}")))?;
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
) -> Result<(), FlowError> {
    if session.outstanding_ping.is_some() {
        return Ok(());
    }

    let request_id = next_request_id(&mut session.next_request_id);
    let ping = Ping {
        id: RequestId { value: request_id },
        timestamp: now_millis()?,
    };
    let payload = encode_ping_payload(&ping);
    send_payload(stream, config, PING_CODE, &payload)?;

    session.outstanding_ping = Some(request_id);
    session.ping_deadline = Some(Instant::now() + config.ping_timeout);
    session.next_ping_at = Instant::now() + config.ping_interval;
    println!("sent ping {}", request_id);
    Ok(())
}

fn send_payload(
    stream: &mut TcpStream,
    config: &NodeConfig,
    payload_code: i32,
    payload_body: &[u8],
) -> Result<(), FlowError> {
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
    *next_request_id = next_request_id.wrapping_add(1);
    if *next_request_id == 0 {
        *next_request_id = 1;
    }
    value
}

fn now_millis() -> Result<i64, FlowError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| FlowError::Protocol(format!("invalid system time: {error}")))?;
    i64::try_from(duration.as_millis())
        .map_err(|error| FlowError::Protocol(format!("timestamp overflow: {error}")))
}
