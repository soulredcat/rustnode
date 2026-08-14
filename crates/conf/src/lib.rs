#![forbid(unsafe_code)]
//! Configuration and persisted P2P identity for the Alephium bot fast-path peer.

use std::env;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use alephium_crypto::{PRIVATE_KEY_LENGTH, PUBLIC_KEY_LENGTH, PrivateKey, PublicKey};

pub const STATUS: &str = "bot fast-path peer configuration";

pub const NETWORK_MAGIC_BYTES: [u8; 4] = [0xa2, 0xfd, 0xc6, 0xbb];
pub const DEFAULT_PEER_ADDRESS: &str = "127.0.0.1:19985";
pub const DEFAULT_IPC_BIND_ADDRESS: &str = "127.0.0.1:19090";
pub const DEFAULT_NETWORK_ID: u8 = 2;
pub const DEFAULT_CONFIGURED_GROUPS: u32 = 4;
pub const DEFAULT_BROKER_ID: i32 = 0;
pub const DEFAULT_BROKER_NUM: i32 = 1;
pub const CLIENT_ID: &str = "rust-alephium/v4.0.0/linux/p2p-v1";

pub const DEFAULT_PING_INTERVAL: Duration = Duration::from_secs(5);
pub const DEFAULT_PING_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEFAULT_IO_POLL_INTERVAL: Duration = Duration::from_millis(5);
pub const DEFAULT_TX_REQUEST_TIMEOUT: Duration = Duration::from_millis(250);
pub const DEFAULT_MAX_FRAME_DATA: usize = 1024 * 1024;
pub const DEFAULT_TX_REQUEST_BATCH_SIZE: usize = 1;
pub const DEFAULT_MAX_INFLIGHT_TX_REQUESTS: usize = 8;
pub const DEFAULT_MAX_TRACKED_TX_IDS: usize = 65_536;
pub const DEFAULT_MAX_TX_REQUEST_RETRIES: u8 = 2;
pub const DEFAULT_EVENT_QUEUE_CAPACITY: usize = 4096;

pub const NODE_IDENTITY_PATH_ENV: &str = "RUSTNODE_IDENTITY_PATH";
pub const DEFAULT_NODE_IDENTITY_PATH: &str = ".local/rustnode/node-identity.bin";

/// Deterministic compatibility identity retained from the original interop harness.
/// It is a P2P identity only and must never be funded or reused as a wallet key.
pub const PRIVATE_KEY_BYTES: [u8; 32] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
];

pub const CLIQUE_ID_BYTES: [u8; 33] = [
    0x02, 0x79, 0xbe, 0x66, 0x7e, 0xf9, 0xdc, 0xbb, 0xac, 0x55, 0xa0, 0x62, 0x95, 0xce, 0x87, 0x0b,
    0x07, 0x02, 0x9b, 0xfc, 0xdb, 0x2d, 0xce, 0x28, 0xd9, 0x59, 0xf2, 0x81, 0x5b, 0x16, 0xf8, 0x17,
    0x98,
];

const NODE_IDENTITY_BYTES: usize = PRIVATE_KEY_LENGTH + PUBLIC_KEY_LENGTH;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeIdentity {
    pub private_key: [u8; PRIVATE_KEY_LENGTH],
    pub clique_id: [u8; PUBLIC_KEY_LENGTH],
}

impl NodeIdentity {
    #[must_use]
    pub const fn compatibility_fixture() -> Self {
        Self {
            private_key: PRIVATE_KEY_BYTES,
            clique_id: CLIQUE_ID_BYTES,
        }
    }
}

#[derive(Debug)]
pub enum NodeIdentityError {
    Io(io::Error),
    InvalidStorage(String),
    InvalidPrivateKey(String),
    InvalidCliqueId(String),
}

impl fmt::Display for NodeIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "node identity I/O error: {error}"),
            Self::InvalidStorage(details) => write!(formatter, "invalid node identity: {details}"),
            Self::InvalidPrivateKey(details) => write!(formatter, "invalid private key: {details}"),
            Self::InvalidCliqueId(details) => write!(formatter, "invalid clique id: {details}"),
        }
    }
}

impl std::error::Error for NodeIdentityError {}

impl From<io::Error> for NodeIdentityError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub peer_address: String,
    pub ipc_bind_address: String,
    pub network_magic: [u8; 4],
    pub configured_groups: u32,
    pub client_id: String,
    pub broker_id: i32,
    pub broker_num: i32,
    pub private_key: [u8; PRIVATE_KEY_LENGTH],
    pub clique_id: [u8; PUBLIC_KEY_LENGTH],
    pub ping_interval: Duration,
    pub ping_timeout: Duration,
    pub io_poll_interval: Duration,
    pub tx_request_timeout: Duration,
    pub max_frame_data: usize,
    pub tx_request_batch_size: usize,
    pub max_inflight_tx_requests: usize,
    pub max_tracked_tx_ids: usize,
    pub max_tx_request_retries: u8,
    pub event_queue_capacity: usize,
    pub network_id: u8,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            peer_address: DEFAULT_PEER_ADDRESS.to_owned(),
            ipc_bind_address: DEFAULT_IPC_BIND_ADDRESS.to_owned(),
            network_magic: NETWORK_MAGIC_BYTES,
            configured_groups: DEFAULT_CONFIGURED_GROUPS,
            client_id: CLIENT_ID.to_owned(),
            broker_id: DEFAULT_BROKER_ID,
            broker_num: DEFAULT_BROKER_NUM,
            private_key: PRIVATE_KEY_BYTES,
            clique_id: CLIQUE_ID_BYTES,
            ping_interval: DEFAULT_PING_INTERVAL,
            ping_timeout: DEFAULT_PING_TIMEOUT,
            io_poll_interval: DEFAULT_IO_POLL_INTERVAL,
            tx_request_timeout: DEFAULT_TX_REQUEST_TIMEOUT,
            max_frame_data: DEFAULT_MAX_FRAME_DATA,
            tx_request_batch_size: DEFAULT_TX_REQUEST_BATCH_SIZE,
            max_inflight_tx_requests: DEFAULT_MAX_INFLIGHT_TX_REQUESTS,
            max_tracked_tx_ids: DEFAULT_MAX_TRACKED_TX_IDS,
            max_tx_request_retries: DEFAULT_MAX_TX_REQUEST_RETRIES,
            event_queue_capacity: DEFAULT_EVENT_QUEUE_CAPACITY,
            network_id: DEFAULT_NETWORK_ID,
        }
    }
}

impl NodeConfig {
    #[must_use]
    pub fn with_peer_address(peer_address: impl Into<String>) -> Self {
        Self {
            peer_address: peer_address.into(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn set_peer_address(mut self, peer_address: impl Into<String>) -> Self {
        self.peer_address = peer_address.into();
        self
    }

    #[must_use]
    pub fn set_ipc_bind_address(mut self, ipc_bind_address: impl Into<String>) -> Self {
        self.ipc_bind_address = ipc_bind_address.into();
        self
    }

    #[must_use]
    pub fn set_max_inflight_tx_requests(mut self, value: usize) -> Self {
        self.max_inflight_tx_requests = value.max(1);
        self
    }

    #[must_use]
    pub fn set_tx_request_batch_size(mut self, value: usize) -> Self {
        self.tx_request_batch_size = value.clamp(1, DEFAULT_TX_REQUEST_BATCH_SIZE);
        self
    }

    #[must_use]
    pub fn set_tx_request_timeout(mut self, value: Duration) -> Self {
        self.tx_request_timeout = value.max(Duration::from_millis(1));
        self
    }

    #[must_use]
    pub fn set_event_queue_capacity(mut self, value: usize) -> Self {
        self.event_queue_capacity = value.max(1);
        self
    }

    #[must_use]
    pub fn with_node_identity(mut self, identity: NodeIdentity) -> Self {
        self.private_key = identity.private_key;
        self.clique_id = identity.clique_id;
        self
    }

    pub fn with_node_identity_path(
        self,
        path: impl AsRef<Path>,
    ) -> Result<Self, NodeIdentityError> {
        let identity = load_or_create_node_identity_at(path)?;
        Ok(self.with_node_identity(identity))
    }

    #[must_use]
    pub const fn configured_groups(&self) -> u32 {
        self.configured_groups
    }
}

#[must_use]
pub fn resolve_node_identity_path() -> PathBuf {
    env::var(NODE_IDENTITY_PATH_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_NODE_IDENTITY_PATH))
}

pub fn load_or_create_node_identity_at(
    path: impl AsRef<Path>,
) -> Result<NodeIdentity, NodeIdentityError> {
    let path = path.as_ref();
    if path.exists() {
        return read_node_identity(path);
    }

    let identity = NodeIdentity::compatibility_fixture();
    write_node_identity(path, identity)?;
    Ok(identity)
}

fn read_node_identity(path: &Path) -> Result<NodeIdentity, NodeIdentityError> {
    let mut bytes = Vec::new();
    File::open(path)?.read_to_end(&mut bytes)?;
    if bytes.len() != NODE_IDENTITY_BYTES {
        return Err(NodeIdentityError::InvalidStorage(format!(
            "expected {NODE_IDENTITY_BYTES} bytes, found {}",
            bytes.len()
        )));
    }

    let mut private_key = [0u8; PRIVATE_KEY_LENGTH];
    let mut clique_id = [0u8; PUBLIC_KEY_LENGTH];
    private_key.copy_from_slice(&bytes[..PRIVATE_KEY_LENGTH]);
    clique_id.copy_from_slice(&bytes[PRIVATE_KEY_LENGTH..]);

    PrivateKey::try_from(private_key.as_slice())
        .map_err(|error| NodeIdentityError::InvalidPrivateKey(error.to_string()))?;
    PublicKey::try_from(clique_id.as_slice())
        .map_err(|error| NodeIdentityError::InvalidCliqueId(error.to_string()))?;

    Ok(NodeIdentity {
        private_key,
        clique_id,
    })
}

fn write_node_identity(path: &Path, identity: NodeIdentity) -> Result<(), NodeIdentityError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }

    let tmp = temporary_path_for(path)?;
    {
        let mut file = File::create(&tmp)?;
        file.write_all(&identity.private_key)?;
        file.write_all(&identity.clique_id)?;
        file.sync_all()?;
    }

    set_restrictive_permissions(&tmp)?;
    fs::rename(&tmp, path)?;
    set_restrictive_permissions(path)?;
    Ok(())
}

fn temporary_path_for(path: &Path) -> io::Result<PathBuf> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("node-identity.bin");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    let mut tmp = path.to_path_buf();
    tmp.set_file_name(format!(".{file_name}.tmp-{nanos}"));
    Ok(tmp)
}

fn set_restrictive_permissions(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
