// SPDX-License-Identifier: GPL-2.0
//! Host listener/backlog state used by the same safe Vsock backend. The Linux
//! File adapter receives cleanup tickets only after an accepted flow is ready.
use crate::{
    host_api::{into_delivery, Consumer, Producer},
    queue::RequestError,
    service::Resource,
    vsock::{AttemptId, Flow},
};
pub(super) const LISTENERS: usize = 32;
const BACKLOG: usize = 16;
pub(super) struct Incoming {
    pub flow: Flow,
    pub attempt: AttemptId,
    pub endpoints: Option<(Producer, Consumer)>,
}
pub(super) struct Listener {
    pub id: u64,
    pub context: usize,
    pub port: u32,
    pub closed: bool,
    backlog: usize,
    incoming: [Option<Incoming>; BACKLOG],
}
impl Listener {
    pub fn new(id: u64, context: usize, port: u32, backlog: usize) -> Self {
        const EMPTY: Option<Incoming> = None;
        Self {
            id,
            context,
            port,
            closed: false,
            backlog: backlog.min(BACKLOG),
            incoming: [EMPTY; BACKLOG],
        }
    }
    pub fn set_backlog(&mut self, n: usize) {
        self.backlog = n.min(BACKLOG)
    }
    pub fn reserve(&mut self, flow: Flow, attempt: AttemptId) -> Result<(), RequestError> {
        if self.closed || self.incoming.iter().flatten().count() >= self.backlog {
            return Err(RequestError::ResourceLimit);
        }
        let slot = self
            .incoming
            .iter_mut()
            .find(|s| s.is_none())
            .ok_or(RequestError::ResourceLimit)?;
        *slot = Some(Incoming {
            flow,
            attempt,
            endpoints: None,
        });
        Ok(())
    }
    pub fn establish(
        &mut self,
        flow: Flow,
        attempt: AttemptId,
        tx: Producer,
        rx: Consumer,
    ) -> bool {
        if self.closed {
            return false;
        }
        if let Some(pending) = self
            .incoming
            .iter_mut()
            .flatten()
            .find(|i| i.flow == flow && i.attempt == attempt)
        {
            pending.endpoints = Some((tx, rx));
            true
        } else {
            false
        }
    }
    pub fn reset(&mut self, flow: Flow, attempt: AttemptId) {
        for item in &mut self.incoming {
            if item.as_ref().map_or(false, |i| {
                (i.flow == flow || i.flow == flow.reversed()) && i.attempt == attempt
            }) {
                item.take();
            }
        }
    }
    pub fn ready(&self) -> bool {
        self.incoming
            .iter()
            .flatten()
            .any(|i| i.endpoints.is_some())
    }
    pub fn accept(&mut self) -> Option<(Flow, AttemptId, [Resource; 2])> {
        let slot = self
            .incoming
            .iter()
            .position(|i| i.as_ref().map_or(false, |i| i.endpoints.is_some()))?;
        let incoming = self.incoming[slot].take().unwrap();
        let (tx, rx) = incoming.endpoints.unwrap();
        Some((incoming.flow, incoming.attempt, into_delivery(tx, rx)))
    }
    pub fn pending_flows(&self) -> impl Iterator<Item = (Flow, AttemptId)> + '_ {
        self.incoming.iter().flatten().map(|i| (i.flow, i.attempt))
    }
}
