#![forbid(unsafe_code)]
//! M1-compatible node and network configuration.

use std::time::Duration;

/// Current implementation status.
pub const STATUS: &str = "M1 local bootstrap configuration implemented";

/// Deterministic local test configuration used for official-node interoperability.
pub const NETWORK_MAGIC_BYTES: [u8; 4] = [0xa2, 0xfd, 0xc6, 0xbb];

/// Local default peer for the devnet harness.
pub const DEFAULT_PEER_ADDRESS: &str = "127.0.0.1:19973";

/// M1 devnet `network-id` used by the local official node harness.
pub const DEFAULT_NETWORK_ID: u8 = 2;

/// Configured local groups in `config/official-node/user.conf`.
pub const DEFAULT_CONFIGURED_GROUPS: u32 = 4;

/// Deterministic local broker identity (`broker-id` / `broker-num`).
pub const DEFAULT_BROKER_ID: i32 = 0;
pub const DEFAULT_BROKER_NUM: i32 = 1;

/// Client identifier used by the deterministic M1 test node.
pub const CLIENT_ID: &str = "rust-alephium/v4.0.0/linux/p2p-v1";

/// Deterministic identity fixture from the Scala vector generator.
pub const PRIVATE_KEY_BYTES: [u8; 32] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
];

/// Deterministic fixture clique identity.
pub const CLIQUE_ID_BYTES: [u8; 33] = [
    0x02, 0x79, 0xbe, 0x66, 0x7e, 0xf9, 0xdc, 0xbb, 0xac, 0x55, 0xa0, 0x62,
    0x95, 0xce, 0x87, 0x0b, 0x07, 0x02, 0x9b, 0xfc, 0xdb, 0x2d, 0xce, 0x28, 0xd9,
    0x59, 0xf2, 0x81, 0x5b, 0x16, 0xf8, 0x17, 0x98,
];

/// Liveness pacing for the minimum M1 bootstrap.
pub const DEFAULT_PING_INTERVAL: Duration = Duration::from_secs(5);
pub const DEFAULT_PING_TIMEOUT: Duration = Duration::from_secs(10);

/// Frame payload length cap used by all M1 workspace decoders.
pub const DEFAULT_MAX_FRAME_DATA: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub peer_address: String,
    pub network_magic: [u8; 4],
    pub configured_groups: u32,
    pub client_id: String,
    pub broker_id: i32,
    pub broker_num: i32,
    pub private_key: [u8; 32],
    pub clique_id: [u8; 33],
    pub ping_interval: Duration,
    pub ping_timeout: Duration,
    pub max_frame_data: usize,
    pub network_id: u8,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            peer_address: DEFAULT_PEER_ADDRESS.to_owned(),
            network_magic: NETWORK_MAGIC_BYTES,
            configured_groups: DEFAULT_CONFIGURED_GROUPS,
            client_id: CLIENT_ID.to_owned(),
            broker_id: DEFAULT_BROKER_ID,
            broker_num: DEFAULT_BROKER_NUM,
            private_key: PRIVATE_KEY_BYTES,
            clique_id: CLIQUE_ID_BYTES,
            ping_interval: DEFAULT_PING_INTERVAL,
            ping_timeout: DEFAULT_PING_TIMEOUT,
            max_frame_data: DEFAULT_MAX_FRAME_DATA,
            network_id: DEFAULT_NETWORK_ID,
        }
    }
}

impl NodeConfig {
    #[must_use]
    pub fn local() -> Self {
        Self {
            peer_address: DEFAULT_PEER_ADDRESS.to_owned(),
            network_magic: NETWORK_MAGIC_BYTES,
            configured_groups: DEFAULT_CONFIGURED_GROUPS,
            client_id: CLIENT_ID.to_owned(),
            broker_id: DEFAULT_BROKER_ID,
            broker_num: DEFAULT_BROKER_NUM,
            private_key: PRIVATE_KEY_BYTES,
            clique_id: CLIQUE_ID_BYTES,
            ping_interval: DEFAULT_PING_INTERVAL,
            ping_timeout: DEFAULT_PING_TIMEOUT,
            max_frame_data: DEFAULT_MAX_FRAME_DATA,
            network_id: DEFAULT_NETWORK_ID,
        }
    }

    #[must_use]
    pub fn with_peer_address(peer_address: impl Into<String>) -> Self {
        Self {
            peer_address: peer_address.into(),
            ..Self::default()
        }
    }
}

impl NodeConfig {
    pub fn set_peer_address(mut self, peer_address: impl Into<String>) -> Self {
        self.peer_address = peer_address.into();
        self
    }

    #[must_use]
    pub const fn configured_groups(&self) -> u32 {
        self.configured_groups
    }

    #[must_use]
    pub const fn network_id(&self) -> u8 {
        self.network_id
    }
}
