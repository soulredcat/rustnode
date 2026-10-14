#![forbid(unsafe_code)]
//! Official Alephium P2P framing and the payload subset used by the bot fast
//! path: handshake, liveness, inventory readiness, chain state, transaction
//! announcements, and transaction fetch. Block and header sync payloads are
//! recognized by code only and never decoded.

pub mod message;
