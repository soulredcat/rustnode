#![forbid(unsafe_code)]
//! Alephium networking, synchronization, validation, and BlockFlow ownership.
//!
//! The first implementation slice contains only M1 networking: connect, handshake,
//! and Ping/Pong liveness for direct official-node interoperability.

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
    decode_hello_payload, decode_ping_payload, decode_pong_payload, encode_hello_payload,
    encode_ping_payload, encode_pong_payload, Hello, InterBrokerInfo, Ping, Pong, RequestId,
    HELLO_CODE, PING_CODE, PONG_CODE,
};
use alephium_protocol::message::payload::CliqueId;
use alephium_serde::Error as SerdeError;

/// Current implementation status.
pub const STATUS: &str =
    "Minimal M1 handshake and Ping/Pong stack implemented for outbound bootstrap.";

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

/// Start an outbound M1 bootstrap session and keep it alive until remote close or local failure.
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
            HELLO_CODE => handle_hello(config, session, payload)?,
            PING_CODE => {
                let ping = decode_ping_payload(payload)?.finish(payload_len)?;
                handle_ping(stream, config, session, ping)?;
            }
            PONG_CODE => {
                let pong = decode_pong_payload(payload)?.finish(payload_len)?;
                handle_pong(session, pong)?;
            }
            unsupported => match session.state {
                SessionState::Established => {
                    println!(
                        "received unsupported payload code {unsupported} in Established state"
                    );
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

fn handle_hello(config: &NodeConfig, session: &mut Session, payload: &[u8]) -> Result<(), FlowError> {
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
    Ok(())
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
