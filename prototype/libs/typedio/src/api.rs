// SPDX-License-Identifier: MPL-2.0
// Shared module graph, included by Cargo and the Linux Rust crate.
pub mod abi;
pub mod flow;
pub mod irq;
pub mod lifetime;
pub mod message;
pub mod notification;
pub mod pipe;
pub mod queue;
pub mod resource;
pub mod schema;
pub mod service;
pub mod vsock;

pub use message::{BoundedVec, Message, Protocol, ProtocolId};
pub use schema::SchemaDigest;
pub use typedio_macros::Message;
