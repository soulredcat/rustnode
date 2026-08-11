#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::{Duration, Instant};

use alephium_conf::{NodeConfig, NodeIdentity};
use alephium_crypto::{sign, PrivateKey, PublicKey};
use alephium_flow::run;
use alephium_protocol::message::{
    decode_frame_prefix, decode_raw_message_data, encode_frame, encode_raw_message_data, FrameDecode,
    NetworkMagic,
};
use alephium_protocol::message::payload::{
    decode_blocks_request_payload, decode_headers_request_payload, decode_hello_payload,
    decode_inv_request_payload, decode_ping_payload, decode_txs_request_payload,
    encode_blocks_response_payload, encode_chain_state_payload, encode_headers_response_payload,
    encode_hello_payload, encode_inv_response_payload, encode_pong_payload,
    encode_txs_response_payload, BlockHash, ChainState, ChainTip, CliqueId, HeadersResponse,
    Hello, InterBrokerInfo, InvResponse, RequestId, TxsResponse,
    BLOCKS_REQUEST_CODE, BLOCKS_RESPONSE_CODE, CHAIN_STATE_CODE, HEADERS_REQUEST_CODE,
    HEADERS_RESPONSE_CODE, HELLO_CODE, INV_REQUEST_CODE, INV_RESPONSE_CODE, PING_CODE,
    PONG_CODE, TXS_REQUEST_CODE, TXS_RESPONSE_CODE,
};
use alephium_protocol::message::payload::Pong as PongPayload;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MockEvent {
    ChainStateSent,
    HelloSent,
    HelloReceived,
    InvRequestReceived,
    InvResponseSent,
    HeadersRequestReceived,
    BlocksRequestReceived,
    TxsRequestReceived,
    HeadersResponseSent,
    BlocksResponseSent,
    TxsResponseSent,
    PingReceived,
    PongSent,
    Completed,
}

#[derive(Debug, Clone, Copy)]
enum InvPayloadKind {
    Empty,
    NonEmpty,
}

#[test]
fn full_rust_mock_peer_handshake_and_ping_pong() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock peer");
    let listener_addr = listener.local_addr().expect("peer address");

    let (tx, rx) = mpsc::channel::<MockEvent>();

    let server = thread::spawn({
        let config = base_config();
        move || {
            let (stream, _) = listener.accept().expect("mock peer accepted");
            mock_peer_with_inv_behavior(
                stream,
                config,
                tx,
                InvPayloadKind::NonEmpty,
            )
            .expect("mock peer completed");
        }
    });

    let mut client_config = base_config().with_node_identity(NodeIdentity::fixture());
    client_config.peer_address = listener_addr.to_string();
    client_config.ping_interval = Duration::from_millis(20);
    client_config.ping_timeout = Duration::from_millis(300);

    let client = thread::spawn(move || run(&client_config));

    let result = client.join().expect("client thread should not panic");
    server.join().expect("server thread should not panic");

    let mut events = HashSet::new();
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(25)) {
            Ok(event) => {
                println!("interop event: {:?}", event);
                events.insert(event);
                if event == MockEvent::Completed {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    assert!(events.contains(&MockEvent::ChainStateSent), "server should send chain state");
    assert!(events.contains(&MockEvent::HelloSent), "server should send hello");
    assert!(events.contains(&MockEvent::HelloReceived), "client should send hello to server");
    assert!(
        events.contains(&MockEvent::InvRequestReceived),
        "client should send InvRequest probe"
    );
    assert!(
        events.contains(&MockEvent::HeadersRequestReceived),
        "client should send HeadersRequest probe"
    );
    assert!(
        events.contains(&MockEvent::BlocksRequestReceived),
        "client should send BlocksRequest probe"
    );
    assert!(
        events.contains(&MockEvent::TxsRequestReceived),
        "client should send TxsRequest probe"
    );
    assert!(
        events.contains(&MockEvent::HeadersResponseSent),
        "server should send HeadersResponse"
    );
    assert!(
        events.contains(&MockEvent::BlocksResponseSent),
        "server should send BlocksResponse"
    );
    assert!(
        events.contains(&MockEvent::TxsResponseSent),
        "server should send TxsResponse"
    );
    assert!(events.contains(&MockEvent::PingReceived), "mock peer should receive client ping");
    assert!(events.contains(&MockEvent::PongSent), "mock peer should send pong response");

    assert!(
        result.is_err(),
        "mock peer closes socket after probe exchange, expected client termination"
    );
}

#[test]
fn full_rust_mock_peer_empty_inv_response_uses_chain_state_locator_fallback() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock peer");
    let listener_addr = listener.local_addr().expect("peer address");

    let (tx, rx) = mpsc::channel::<MockEvent>();

    let server = thread::spawn({
        let config = base_config();
        move || {
            let (stream, _) = listener.accept().expect("mock peer accepted");
            mock_peer_with_inv_behavior(
                stream,
                config,
                tx,
                InvPayloadKind::Empty,
            )
            .expect("mock peer completed");
        }
    });

    let mut client_config = base_config().with_node_identity(NodeIdentity::fixture());
    client_config.peer_address = listener_addr.to_string();
    client_config.ping_interval = Duration::from_millis(20);
    client_config.ping_timeout = Duration::from_millis(300);

    let client = thread::spawn(move || run(&client_config));

    let result = client.join().expect("client thread should not panic");
    server.join().expect("server thread should not panic");

    let mut events = HashSet::new();
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(25)) {
            Ok(event) => {
                println!("interop event (empty-inv): {:?}", event);
                events.insert(event);
                if event == MockEvent::Completed {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    assert!(events.contains(&MockEvent::ChainStateSent), "server should send chain state");
    assert!(events.contains(&MockEvent::HelloSent), "server should send hello");
    assert!(events.contains(&MockEvent::HelloReceived), "client should send hello to server");
    assert!(
        events.contains(&MockEvent::InvRequestReceived),
        "client should send InvRequest probe"
    );
    assert!(
        events.contains(&MockEvent::HeadersRequestReceived),
        "client should send HeadersRequest probe"
    );
    assert!(
        events.contains(&MockEvent::BlocksRequestReceived),
        "client should send BlocksRequest probe"
    );
    assert!(
        events.contains(&MockEvent::TxsRequestReceived),
        "client should send TxsRequest probe"
    );
    assert!(
        events.contains(&MockEvent::HeadersResponseSent),
        "server should send HeadersResponse"
    );
    assert!(
        events.contains(&MockEvent::BlocksResponseSent),
        "server should send BlocksResponse"
    );
    assert!(
        events.contains(&MockEvent::TxsResponseSent),
        "server should send TxsResponse"
    );
    assert!(events.contains(&MockEvent::PingReceived), "mock peer should receive client ping");
    assert!(events.contains(&MockEvent::PongSent), "mock peer should send pong response");

    assert!(
        result.is_err(),
        "mock peer closes socket after probe exchange, expected client termination"
    );
}

fn base_config() -> NodeConfig {
    NodeConfig::local().with_node_identity(NodeIdentity::fixture())
}

#[allow(dead_code)]
fn mock_peer(
    stream: TcpStream,
    config: NodeConfig,
    events: Sender<MockEvent>,
) -> io::Result<()> {
    mock_peer_with_inv_behavior(stream, config, events, InvPayloadKind::NonEmpty)
}

fn mock_peer_with_inv_behavior(
    mut stream: TcpStream,
    config: NodeConfig,
    events: Sender<MockEvent>,
    inv_kind: InvPayloadKind,
) -> io::Result<()> {
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_millis(150)))?;
    let magic = NetworkMagic::new(config.network_magic);

    send_chain_state(&mut stream, magic, config.max_frame_data, &events)?;
    send_payload(
        &mut stream,
        magic,
        config.max_frame_data,
        HELLO_CODE,
        &build_hello_payload(&config),
        &events,
        MockEvent::HelloSent,
    )?;

    let mut inbound = Vec::new();
    let mut read_buffer = [0_u8; 1024];
    let mut saw_inv_request = false;
    let mut saw_headers_request = false;
    let mut saw_blocks_request = false;
    let mut saw_txs_request = false;
    let mut saw_ping = false;
    let deadline = Instant::now() + Duration::from_secs(4);

    while Instant::now() <= deadline {
        match stream.read(&mut read_buffer) {
            Ok(0) => break,
            Ok(read) => inbound.extend_from_slice(&read_buffer[..read]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
            Err(error) if error.kind() == io::ErrorKind::TimedOut => continue,
            Err(error) => return Err(error),
        }

        while let Some((payload_code, payload)) = pop_frame(&mut inbound, magic, config.max_frame_data)? {
            match payload_code {
                HELLO_CODE => {
                    decode_hello_payload(&payload, config.configured_groups())
                        .expect("server validates remote hello");
                    events.send(MockEvent::HelloReceived).ok();
                }
                INV_REQUEST_CODE => {
                    let request = decode_inv_request_payload(&payload)
                        .expect("mock peer decodes InvRequest")
                        .finish(payload.len())
                        .expect("mock peer consumes InvRequest fully");
                    println!("mock peer received InvRequest id={}", request.id.value);
                    match inv_kind {
                        InvPayloadKind::Empty => send_inv_response_empty(
                            &mut stream,
                            magic,
                            config.max_frame_data,
                            &request.id,
                            &events,
                        )?,
                        InvPayloadKind::NonEmpty => send_inv_response(
                            &mut stream,
                            magic,
                            config.max_frame_data,
                            &request.id,
                            &events,
                        )?,
                    }
                    events.send(MockEvent::InvRequestReceived).ok();
                    saw_inv_request = true;
                }
                HEADERS_REQUEST_CODE => {
                    let request = decode_headers_request_payload(&payload)
                        .expect("mock peer decodes HeadersRequest")
                        .finish(payload.len())
                        .expect("mock peer consumes HeadersRequest fully");
                    println!("mock peer received HeadersRequest id={}", request.id.value);
                    send_headers_response(
                        &mut stream,
                        magic,
                        config.max_frame_data,
                        &request.id,
                        &events,
                    )?;
                    events.send(MockEvent::HeadersRequestReceived).ok();
                    saw_headers_request = true;
                }
                BLOCKS_REQUEST_CODE => {
                    let request = decode_blocks_request_payload(&payload)
                        .expect("mock peer decodes BlocksRequest")
                        .finish(payload.len())
                        .expect("mock peer consumes BlocksRequest fully");
                    println!("mock peer received BlocksRequest id={}", request.id.value);
                    send_blocks_response(
                        &mut stream,
                        magic,
                        config.max_frame_data,
                        &request.id,
                        &events,
                    )?;
                    events.send(MockEvent::BlocksRequestReceived).ok();
                    saw_blocks_request = true;
                }
                TXS_REQUEST_CODE => {
                    let request = decode_txs_request_payload(&payload)
                        .expect("mock peer decodes TxsRequest")
                        .finish(payload.len())
                        .expect("mock peer consumes TxsRequest fully");
                    println!("mock peer received TxsRequest id={}", request.id.value);
                    send_txs_response(
                        &mut stream,
                        magic,
                        config.max_frame_data,
                        &request.id,
                        &events,
                    )?;
                    events.send(MockEvent::TxsRequestReceived).ok();
                    saw_txs_request = true;
                }
                PING_CODE => {
                    let ping = decode_ping_payload(&payload)
                        .expect("mock peer decodes Ping")
                        .finish(payload.len())
                        .expect("mock peer consumes Ping fully");
                    println!("mock peer received Ping id={}", ping.id.value);
                    send_payload(
                        &mut stream,
                        magic,
                        config.max_frame_data,
                        PONG_CODE,
                        &encode_pong_payload(&PongPayload { id: ping.id }),
                        &events,
                        MockEvent::PongSent,
                    )?;
                    events.send(MockEvent::PingReceived).ok();
                    saw_ping = true;
                }
                _ => {}
            }
        }

        if saw_inv_request && saw_headers_request && saw_blocks_request && saw_txs_request && saw_ping {
            break;
        }
    }

    events.send(MockEvent::Completed).ok();
    stream.shutdown(Shutdown::Both)?;
    Ok(())
}

fn send_chain_state(
    stream: &mut TcpStream,
    magic: NetworkMagic,
    max_frame_data: usize,
    events: &Sender<MockEvent>,
) -> io::Result<()> {
    let chain_state = ChainState {
        tips: vec![ChainTip {
            hash: BlockHash {
                bytes: [0xAB_u8; 32],
            },
            height: 42,
            weight: vec![0x01_u8, 0x02, 0x03],
        }],
    };
    let payload = encode_chain_state_payload(&chain_state).expect("encode ChainState");
    send_payload(
        stream,
        magic,
        max_frame_data,
        CHAIN_STATE_CODE,
        &payload,
        events,
        MockEvent::ChainStateSent,
    )
}

fn send_inv_response_empty(
    stream: &mut TcpStream,
    magic: NetworkMagic,
    max_frame_data: usize,
    request_id: &RequestId,
    events: &Sender<MockEvent>,
) -> io::Result<()> {
    let response = InvResponse {
        id: *request_id,
        hashes: Vec::new(),
    };
    let payload = encode_inv_response_payload(&response).expect("encode InvResponse");
    send_payload(
        stream,
        magic,
        max_frame_data,
        INV_RESPONSE_CODE,
        &payload,
        events,
        MockEvent::InvResponseSent,
    )
}

fn send_inv_response(
    stream: &mut TcpStream,
    magic: NetworkMagic,
    max_frame_data: usize,
    request_id: &RequestId,
    events: &Sender<MockEvent>,
) -> io::Result<()> {
    let response = InvResponse {
        id: *request_id,
        hashes: vec![vec![BlockHash { bytes: [0x11_u8; 32] }]],
    };
    println!("mock peer sending InvResponse id={}", request_id.value);
    let payload = encode_inv_response_payload(&response).expect("encode InvResponse");
    send_payload(
        stream,
        magic,
        max_frame_data,
        INV_RESPONSE_CODE,
        &payload,
        events,
        MockEvent::InvResponseSent,
    )
}

fn send_headers_response(
    stream: &mut TcpStream,
    magic: NetworkMagic,
    max_frame_data: usize,
    request_id: &RequestId,
    events: &Sender<MockEvent>,
) -> io::Result<()> {
    let response = HeadersResponse {
        id: *request_id,
        headers: vec![vec![0x21_u8, 0x22_u8]],
    };
    println!("mock peer sending HeadersResponse id={}", request_id.value);
    let payload = encode_headers_response_payload(&response).expect("encode HeadersResponse");
    println!(
        "mock peer headers response payload_len={} bytes={:?}",
        payload.len(),
        payload
    );
    send_payload(
        stream,
        magic,
        max_frame_data,
        HEADERS_RESPONSE_CODE,
        &payload,
        events,
        MockEvent::HeadersResponseSent,
    )
}

fn send_blocks_response(
    stream: &mut TcpStream,
    magic: NetworkMagic,
    max_frame_data: usize,
    request_id: &RequestId,
    events: &Sender<MockEvent>,
) -> io::Result<()> {
    let response = alephium_protocol::message::payload::BlocksResponse {
        id: *request_id,
        blocks: vec![vec![0x31_u8, 0x32_u8]],
    };
    println!("mock peer sending BlocksResponse id={}", request_id.value);
    let payload = encode_blocks_response_payload(&response).expect("encode BlocksResponse");
    send_payload(
        stream,
        magic,
        max_frame_data,
        BLOCKS_RESPONSE_CODE,
        &payload,
        events,
        MockEvent::BlocksResponseSent,
    )
}

fn send_txs_response(
    stream: &mut TcpStream,
    magic: NetworkMagic,
    max_frame_data: usize,
    request_id: &RequestId,
    events: &Sender<MockEvent>,
) -> io::Result<()> {
    let response = TxsResponse {
        id: *request_id,
        txs: vec![vec![0x41_u8, 0x42_u8]],
    };
    println!("mock peer sending TxsResponse id={}", request_id.value);
    let payload = encode_txs_response_payload(&response).expect("encode TxsResponse");
    send_payload(
        stream,
        magic,
        max_frame_data,
        TXS_RESPONSE_CODE,
        &payload,
        events,
        MockEvent::TxsResponseSent,
    )
}

fn build_hello_payload(config: &NodeConfig) -> Vec<u8> {
    let private_key = PrivateKey::try_from(&config.private_key[..])
        .expect("mock peer has valid private key fixture");
    let clique_id = PublicKey::try_from(&config.clique_id[..])
        .expect("mock peer has valid clique id fixture");

    let broker_info = InterBrokerInfo {
        clique_id: CliqueId {
            bytes: clique_id.bytes(),
        },
        broker_id: config.broker_id,
        broker_num: config.broker_num,
    };
    let signature = sign(&broker_info.hash(), &private_key).expect("sign mock hello");

    let hello = Hello {
        client_id: config.client_id.clone(),
        timestamp: 1_700_000_000_000,
        broker_info,
        signature: signature.bytes(),
    };

    encode_hello_payload(&hello).expect("encode mock hello")
}

fn send_payload(
    stream: &mut TcpStream,
    magic: NetworkMagic,
    max_frame_data: usize,
    payload_code: i32,
    payload_body: &[u8],
    events: &Sender<MockEvent>,
    event: MockEvent,
) -> io::Result<()> {
    let raw = encode_raw_message_data(payload_code, payload_body).expect("encode raw payload");
    let frame = encode_frame(magic, &raw, max_frame_data).expect("encode frame");
    stream.write_all(&frame)?;
    stream.flush()?;
    events.send(event).ok();
    Ok(())
}

fn pop_frame(
    inbound: &mut Vec<u8>,
    magic: NetworkMagic,
    max_frame_data: usize,
) -> io::Result<Option<(i32, Vec<u8>)>> {
    if inbound.is_empty() {
        return Ok(None);
    }

    match decode_frame_prefix(inbound, magic, max_frame_data) {
        Ok(FrameDecode::NeedMoreData { required_total }) if inbound.len() < required_total => {
            Ok(None)
        }
        Ok(FrameDecode::NeedMoreData { required_total }) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid frame prefix, required_total={required_total}"),
        )),
        Ok(FrameDecode::Complete(frame)) => {
            let raw = decode_raw_message_data(frame.data).expect("valid raw message frame");
            let payload = raw.payload_body.to_vec();
            let payload_code = raw.payload_code;
            inbound.drain(0..frame.consumed);
            Ok(Some((payload_code, payload)))
        }
        Err(error) => Err(io::Error::new(io::ErrorKind::InvalidData, error.to_string())),
    }
}
