// SPDX-License-Identifier: MPL-2.0

//! The design's resource-free Vsock protocol types. Endpoints and event
//! transfer are provided by the resource-aware frontend/host support layer.

use crate::Message;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Message)]
pub struct ContextId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Message)]
pub struct Port(pub u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Message)]
pub struct AttemptId(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Message)]
pub struct Address {
    pub cid: ContextId,
    pub port: Port,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Message)]
pub struct Flow {
    pub source: Address,
    pub destination: Address,
}
impl Flow {
    pub fn reversed(self) -> Self {
        Self {
            source: self.destination,
            destination: self.source,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Message)]
pub enum ShutdownDirection {
    Send,
    Receive,
    Both,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Message)]
pub enum VsockControlKind {
    ConnectOffer,
    ConnectAccept,
    Shutdown(ShutdownDirection),
    Reset,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Message)]
pub struct VsockControlMessage {
    pub flow: Flow,
    pub attempt: AttemptId,
    pub kind: VsockControlKind,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Message)]
pub enum VsockRequest {
    SendControl(VsockControlMessage),
    ReceiveEvent,
}

/// Host-assigned routing identity, permissions and charged resource limits.
#[derive(Clone, Debug, Message)]
pub struct VsockConfig {
    pub cid: ContextId,
    pub max_flows: u32,
    pub pipe_capacity: u64,
    pub max_control_queues: u32,
    pub max_pending_events: u32,
    pub peer_cids: crate::BoundedVec<ContextId, 8>,
    pub port_min: Port,
    pub port_max: Port,
}
/// The same protocol source is compiled against guest and host endpoint
/// leaves; native owner layouts never cross the boundary.
#[derive(Message)]
pub enum VsockReply<Tx: Message, Rx: Message> {
    ControlSent,
    Event(VsockEvent<Tx, Rx>),
}
#[derive(Message)]
pub enum VsockEvent<Tx: Message, Rx: Message> {
    Control(VsockControlMessage),
    Established {
        flow: Flow,
        attempt: AttemptId,
        tx: Tx,
        rx: Rx,
    },
}
pub struct VsockProtocol<Tx, Rx>(core::marker::PhantomData<(Tx, Rx)>);
impl<Tx: Message, Rx: Message> crate::Protocol for VsockProtocol<Tx, Rx> {
    type Config = VsockConfig;
    type Request = VsockRequest;
    type Reply = VsockReply<Tx, Rx>;
    const ID: crate::ProtocolId = crate::ProtocolId(*b"KletTypedVsock01");
}
