// SPDX-License-Identifier: MPL-2.0

//! Safe Vsock backend flow decisions. The backend lock serializes controls,
//! pipe preparation publication, retirement and attachment removal. Internal
//! preparation state is not a new peer handshake message or stage.

use crate::vsock::{
    AttemptId, ContextId, Flow, ShutdownDirection, VsockControlKind, VsockControlMessage,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Offered,
    Preparing,
    Established,
    Retiring,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Permission,
    Invalid,
    Stale,
    Quota,
    Busy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    flow: Flow,
    attempt: AttemptId,
    phase: Phase,
    queued_controls: u32,
    shut: [u8; 2],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    DeliverControl,
    PreparePipes,
    CloseDirections {
        sender: usize,
        directions: ShutdownDirection,
    },
    ResetPipes,
}
pub struct FlowTable<const N: usize> {
    entries: [Option<Entry>; N],
}
impl<const N: usize> Default for FlowTable<N> {
    fn default() -> Self {
        Self::new()
    }
}
impl<const N: usize> FlowTable<N> {
    pub const fn new() -> Self {
        Self { entries: [None; N] }
    }
    /// `source_cid` comes from the framework's authenticated attachment,
    /// never from the control message. The caller checks configured peer/port
    /// rights before entering this state machine and reserves event capacity.
    pub fn control(
        &mut self,
        source_cid: ContextId,
        message: VsockControlMessage,
    ) -> Result<Action, Error> {
        if message.flow.source.cid != source_cid {
            return Err(Error::Permission);
        }
        if message.flow.source == message.flow.destination {
            return Err(Error::Invalid);
        }
        if message.kind == VsockControlKind::ConnectOffer {
            if self.find_pair(message.flow).is_some() {
                return Err(Error::Busy);
            }
            let slot = self
                .entries
                .iter_mut()
                .find(|x| x.is_none())
                .ok_or(Error::Quota)?;
            *slot = Some(Entry {
                flow: message.flow,
                attempt: message.attempt,
                phase: Phase::Offered,
                queued_controls: 0,
                shut: [0; 2],
            });
            return Ok(Action::DeliverControl);
        }
        let e = self.find_pair_mut(message.flow).ok_or(Error::Stale)?;
        if e.attempt != message.attempt || e.phase == Phase::Retiring {
            return Err(Error::Stale);
        }
        match message.kind {
            VsockControlKind::ConnectAccept => {
                if e.phase != Phase::Offered || message.flow != e.flow.reversed() {
                    return Err(Error::Invalid);
                }
                e.phase = Phase::Preparing;
                Ok(Action::PreparePipes)
            }
            VsockControlKind::Shutdown(directions) => {
                if e.phase != Phase::Established {
                    return Err(Error::Invalid);
                }
                let sender = if message.flow == e.flow { 0 } else { 1 };
                e.shut[sender] |= match directions {
                    ShutdownDirection::Send => 1,
                    ShutdownDirection::Receive => 2,
                    ShutdownDirection::Both => 3,
                };
                Ok(Action::CloseDirections { sender, directions })
            }
            VsockControlKind::Reset => {
                e.phase = Phase::Retiring;
                Ok(Action::ResetPipes)
            }
            VsockControlKind::ConnectOffer => unreachable!(),
        }
    }
    /// Call only after two pipes and BOTH peers' event-delivery resources have
    /// been reserved. Checking the same attempt prevents a concurrent Reset
    /// from publishing endpoints belonging to an abandoned preparation.
    pub fn publish_established(
        &mut self,
        flow: Flow,
        attempt: AttemptId,
    ) -> Result<[Flow; 2], Error> {
        let e = self.find_pair_mut(flow).ok_or(Error::Stale)?;
        if e.attempt != attempt || e.phase != Phase::Preparing {
            return Err(Error::Stale);
        }
        e.phase = Phase::Established;
        Ok([e.flow.reversed(), e.flow])
    }
    pub fn preparation_failed(&mut self, flow: Flow, attempt: AttemptId) -> Result<(), Error> {
        let e = self.find_pair_mut(flow).ok_or(Error::Stale)?;
        if e.attempt != attempt || e.phase != Phase::Preparing {
            return Err(Error::Stale);
        }
        e.phase = Phase::Retiring;
        Ok(())
    }
    pub fn retain_control(&mut self, flow: Flow, attempt: AttemptId) -> Result<(), Error> {
        let e = self.find_pair_mut(flow).ok_or(Error::Stale)?;
        if e.attempt != attempt || e.phase == Phase::Retiring {
            return Err(Error::Stale);
        }
        e.queued_controls = e.queued_controls.checked_add(1).ok_or(Error::Quota)?;
        Ok(())
    }
    pub fn resolve_control(&mut self, flow: Flow, attempt: AttemptId) -> Result<(), Error> {
        let e = self.find_pair_mut(flow).ok_or(Error::Stale)?;
        if e.attempt != attempt {
            return Err(Error::Stale);
        }
        e.queued_controls = e.queued_controls.checked_sub(1).ok_or(Error::Invalid)?;
        Ok(())
    }
    /// Backend teardown first resets pipe controls and resolves queued work;
    /// only then can it free a retired slot and permit address-pair reuse.
    pub fn release_retired(&mut self, flow: Flow, attempt: AttemptId) -> Result<(), Error> {
        let index = self
            .entries
            .iter()
            .position(|e| e.as_ref().is_some_and(|e| same_pair(e.flow, flow)))
            .ok_or(Error::Stale)?;
        let e = self.entries[index].as_ref().unwrap();
        if e.attempt != attempt {
            return Err(Error::Stale);
        }
        if e.phase != Phase::Retiring || e.queued_controls != 0 {
            return Err(Error::Busy);
        }
        self.entries[index] = None;
        Ok(())
    }
    pub fn phase(&self, flow: Flow, attempt: AttemptId) -> Option<Phase> {
        self.find_pair(flow)
            .filter(|e| e.attempt == attempt)
            .map(|e| e.phase)
    }
    fn find_pair(&self, flow: Flow) -> Option<&Entry> {
        self.entries
            .iter()
            .flatten()
            .find(|e| same_pair(e.flow, flow))
    }
    fn find_pair_mut(&mut self, flow: Flow) -> Option<&mut Entry> {
        self.entries
            .iter_mut()
            .flatten()
            .find(|e| same_pair(e.flow, flow))
    }
}
fn same_pair(a: Flow, b: Flow) -> bool {
    a == b || a == b.reversed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vsock::{Address, Port};
    fn flow() -> Flow {
        Flow {
            source: Address {
                cid: ContextId(42),
                port: Port(5000),
            },
            destination: Address {
                cid: ContextId(7),
                port: Port(8000),
            },
        }
    }
    fn control(flow: Flow, attempt: u64, kind: VsockControlKind) -> VsockControlMessage {
        VsockControlMessage {
            flow,
            attempt: AttemptId(attempt),
            kind,
        }
    }
    #[test]
    fn handshake_uses_only_offer_accept_and_established() {
        let f = flow();
        let mut t = FlowTable::<1>::new();
        assert_eq!(
            t.control(f.source.cid, control(f, 1, VsockControlKind::ConnectOffer)),
            Ok(Action::DeliverControl)
        );
        assert_eq!(
            t.control(f.source.cid, control(f, 1, VsockControlKind::ConnectAccept)),
            Err(Error::Invalid)
        );
        assert_eq!(
            t.control(
                f.destination.cid,
                control(f.reversed(), 1, VsockControlKind::ConnectAccept)
            ),
            Ok(Action::PreparePipes)
        );
        assert_eq!(
            t.publish_established(f, AttemptId(1)),
            Ok([f.reversed(), f])
        );
        assert_eq!(t.phase(f, AttemptId(1)), Some(Phase::Established));
    }
    #[test]
    fn reset_racing_pipe_preparation_prevents_publication() {
        let f = flow();
        let mut t = FlowTable::<1>::new();
        t.control(f.source.cid, control(f, 1, VsockControlKind::ConnectOffer))
            .unwrap();
        t.control(
            f.destination.cid,
            control(f.reversed(), 1, VsockControlKind::ConnectAccept),
        )
        .unwrap();
        t.control(f.source.cid, control(f, 1, VsockControlKind::Reset))
            .unwrap();
        assert_eq!(t.publish_established(f, AttemptId(1)), Err(Error::Stale));
    }
    #[test]
    fn address_reuse_waits_for_queued_work_and_late_controls_are_stale() {
        let f = flow();
        let mut t = FlowTable::<1>::new();
        t.control(f.source.cid, control(f, 1, VsockControlKind::ConnectOffer))
            .unwrap();
        t.retain_control(f, AttemptId(1)).unwrap();
        t.control(f.source.cid, control(f, 1, VsockControlKind::Reset))
            .unwrap();
        assert_eq!(t.release_retired(f, AttemptId(1)), Err(Error::Busy));
        t.resolve_control(f, AttemptId(1)).unwrap();
        t.release_retired(f, AttemptId(1)).unwrap();
        t.control(f.source.cid, control(f, 2, VsockControlKind::ConnectOffer))
            .unwrap();
        assert_eq!(
            t.control(
                f.destination.cid,
                control(f.reversed(), 1, VsockControlKind::ConnectAccept)
            ),
            Err(Error::Stale)
        );
        assert_eq!(
            t.control(ContextId(99), control(f, 2, VsockControlKind::Reset)),
            Err(Error::Permission)
        );
    }
}
