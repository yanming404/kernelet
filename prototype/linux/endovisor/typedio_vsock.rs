// SPDX-License-Identifier: GPL-2.0
//! Safe Vsock policy over the generic HostRequest and byte-pipe framework.
//! Workers keep bounded receive/event records and never wait for a frontend.
use crate::host_api::{
    self, Backend, Consumer, HostRequest, PipeAccount, PipeControl, Producer, Serialized,
};
use crate::{
    flow::{Action, FlowTable},
    message,
    queue::RequestError,
    service::{Descriptor, Record},
    vsock::*,
    Message,
};
use core::ffi::c_void;
type Reply = VsockReply<Producer, Consumer>;
type Event = VsockEvent<Producer, Consumer>;
type Protocol = VsockProtocol<Producer, Consumer>;
const CONTEXTS: usize = 16;
const FLOWS: usize = 128;
const EVENTS: usize = 16;
const RECEIVES: usize = 8;
struct Context {
    generation: u64,
    config: VsockConfig,
    account: PipeAccount,
    receive: [Option<HostRequest<VsockRequest>>; RECEIVES],
    events: [Option<Event>; EVENTS],
    queues: [Option<(u64, bool)>; 8],
    removed: bool,
    host: bool,
}
impl Context {
    fn event_slot(&self) -> Option<usize> {
        self.events
            .iter()
            .take(self.config.max_pending_events as usize)
            .position(Option::is_none)
    }
    fn flow_allowed(&self, flow: Flow) -> bool {
        flow.source.cid == self.config.cid
            && flow.source.port.0 >= self.config.port_min.0
            && flow.source.port.0 <= self.config.port_max.0
            && self
                .config
                .peer_cids
                .iter()
                .any(|cid| *cid == flow.destination.cid)
    }
    fn queue_allowed(&mut self, generation: u64, receive: bool) -> bool {
        for queue in &mut self.queues {
            if queue
                .as_ref()
                .map_or(false, |(q, _)| !host_api::queue_alive(self.generation, *q))
            {
                *queue = None;
            }
        }
        if let Some((_, kind)) = self.queues.iter().flatten().find(|(q, _)| *q == generation) {
            return *kind == receive;
        }
        if !receive
            && self.queues.iter().flatten().filter(|(_, r)| !*r).count()
                >= self.config.max_control_queues as usize
        {
            return false;
        }
        if let Some(slot) = self.queues.iter_mut().find(|q| q.is_none()) {
            *slot = Some((generation, receive));
            true
        } else {
            false
        }
    }
    fn drain(&mut self) {
        for receive in &mut self.receive {
            if receive.as_ref().map_or(false, |r| r.cancelled()) {
                receive
                    .take()
                    .unwrap()
                    .finish::<Reply>(Err(RequestError::Cancelled));
            }
        }
        loop {
            let ri = self.receive.iter().position(Option::is_some);
            let ei = self.events.iter().position(Option::is_some);
            match (ri, ei) {
                (Some(r), Some(e)) => self.receive[r]
                    .take()
                    .unwrap()
                    .finish(Ok(Reply::Event(self.events[e].take().unwrap()))),
                _ => break,
            }
        }
    }
}
struct FlowData {
    flow: Flow,
    attempt: AttemptId,
    a: usize,
    b: usize,
    owners: [u64; 2],
    pipes: Option<[PipeControl; 2]>,
    terminal: [bool; 2],
    shut: [u8; 2],
}
struct State {
    polling: bool,
    next_listener: u64,
    listeners:
        [Option<crate::typedio_host_socket::Listener>; crate::typedio_host_socket::LISTENERS],
    contexts: [Option<Context>; CONTEXTS],
    table: FlowTable<FLOWS>,
    flows: [Option<FlowData>; FLOWS],
}
impl State {
    const fn new() -> Self {
        const C: Option<Context> = None;
        const F: Option<FlowData> = None;
        const L: Option<crate::typedio_host_socket::Listener> = None;
        Self {
            polling: false,
            next_listener: 1,
            listeners: [L; crate::typedio_host_socket::LISTENERS],
            contexts: [C; CONTEXTS],
            table: FlowTable::new(),
            flows: [F; FLOWS],
        }
    }
    fn add(&mut self, descriptor: &Descriptor, generation: u64) -> Result<usize, RequestError> {
        let config: VsockConfig = message::decode(
            VsockConfig::SCHEMA,
            &descriptor.config[..descriptor.config_len as usize],
        )
        .map_err(|_| RequestError::Invalid)?;
        let host = descriptor.features & 1 != 0;
        if (host && config.cid.0 != 2)
            || (!host && config.cid.0 <= 2)
            || config.max_flows == 0
            || config.max_flows > 16
            || config.pipe_capacity == 0
            || config.pipe_capacity > 1024 * 1024
            || config.max_control_queues == 0
            || config.max_control_queues > 4
            || config.max_pending_events == 0
            || config.max_pending_events > EVENTS as u32
            || config.port_min.0 > config.port_max.0
        {
            return Err(RequestError::Invalid);
        }
        if self
            .contexts
            .iter()
            .flatten()
            .any(|c| c.config.cid == config.cid)
        {
            return Err(RequestError::Permission);
        }
        let slot = self
            .contexts
            .iter()
            .position(Option::is_none)
            .ok_or(RequestError::ResourceLimit)?;
        let bytes = host_api::account_budget(config.pipe_capacity, 2 * config.max_flows);
        let account = PipeAccount::new(generation | (1 << 63), bytes, 2 * config.max_flows)?;
        const R: Option<HostRequest<VsockRequest>> = None;
        const E: Option<Event> = None;
        self.contexts[slot] = Some(Context {
            generation,
            config,
            account,
            receive: [R; RECEIVES],
            events: [E; EVENTS],
            queues: [None; 8],
            removed: false,
            host,
        });
        Ok(slot)
    }
    fn poll(&mut self) {
        if self.polling {
            return;
        }
        self.polling = true;
        self.process_host_events();
        for context in self.contexts.iter_mut().flatten() {
            context.drain()
        }
        for i in 0..FLOWS {
            let Some(flow) = self.flows[i].as_mut() else {
                continue;
            };
            if flow
                .pipes
                .as_ref()
                .map_or(false, |pipes| pipes.iter().all(PipeControl::quiescent))
            {
                flow.pipes.take();
                let _ = self.table.control(
                    flow.flow.source.cid,
                    VsockControlMessage {
                        flow: flow.flow,
                        attempt: flow.attempt,
                        kind: VsockControlKind::Reset,
                    },
                );
            }
            for side in 0..2 {
                if !flow.terminal[side] {
                    continue;
                }
                let target = if side == 0 { flow.a } else { flow.b };
                let Some(context) = self.contexts[target].as_mut() else {
                    flow.terminal[side] = false;
                    continue;
                };
                if context.removed || context.generation != flow.owners[side] {
                    flow.terminal[side] = false;
                    continue;
                }
                if let Some(slot) = context.event_slot() {
                    let direction = if side == 0 {
                        flow.flow.reversed()
                    } else {
                        flow.flow
                    };
                    context.events[slot] = Some(Event::Control(VsockControlMessage {
                        flow: direction,
                        attempt: flow.attempt,
                        kind: VsockControlKind::Reset,
                    }));
                    flow.terminal[side] = false;
                    context.drain();
                }
            }
            if flow.pipes.is_none()
                && flow.terminal == [false; 2]
                && self.table.phase(flow.flow, flow.attempt) == Some(crate::flow::Phase::Retiring)
            {
                let _ = self.table.release_retired(flow.flow, flow.attempt);
                self.flows[i] = None;
            }
        }
        self.process_host_events();
        for context in self.contexts.iter_mut().flatten() {
            context.drain()
        }
        self.polling = false;
    }
    fn process_host_events(&mut self) {
        extern "C" {
            fn klet_tio_host_wake(owner: u64);
        }
        for _ in 0..FLOWS {
            let mut selected = None;
            for ci in 0..CONTEXTS {
                if let Some(c) = self.contexts[ci].as_mut() {
                    if c.host && !c.removed {
                        if let Some(e) = c.events.iter().position(Option::is_some) {
                            selected = Some((ci, c.events[e].take().unwrap()));
                            break;
                        }
                    }
                }
            }
            let Some((ci, event)) = selected else { break };
            match event {
                Event::Control(message) => match message.kind {
                    VsockControlKind::ConnectOffer => {
                        let listener = self.listeners.iter().position(|l| {
                            l.as_ref().map_or(false, |l| {
                                !l.closed
                                    && l.context == ci
                                    && l.port == message.flow.destination.port.0
                            })
                        });
                        let admitted = listener
                            .and_then(|li| {
                                self.listeners[li]
                                    .as_mut()
                                    .unwrap()
                                    .reserve(message.flow, message.attempt)
                                    .ok()
                            })
                            .is_some();
                        if admitted {
                            let accept = VsockControlMessage {
                                flow: message.flow.reversed(),
                                attempt: message.attempt,
                                kind: VsockControlKind::ConnectAccept,
                            };
                            if self.control(ci, accept).is_err() {
                                if let Some(index) = self.flows.iter().position(|f| {
                                    f.as_ref().map_or(false, |f| {
                                        same(f.flow, message.flow) && f.attempt == message.attempt
                                    })
                                }) {
                                    self.reset(index)
                                }
                            }
                        } else if let Some(index) = self.flows.iter().position(|f| {
                            f.as_ref().map_or(false, |f| {
                                same(f.flow, message.flow) && f.attempt == message.attempt
                            })
                        }) {
                            self.reset(index)
                        }
                    }
                    VsockControlKind::Reset => {
                        for listener in self.listeners.iter_mut().flatten() {
                            if listener.context == ci {
                                listener.reset(message.flow, message.attempt)
                            }
                        }
                    }
                    _ => (),
                },
                Event::Established {
                    flow,
                    attempt,
                    tx,
                    rx,
                } => {
                    let listener = self.listeners.iter_mut().flatten().find(|l| {
                        !l.closed && l.context == ci && l.port == flow.destination.port.0
                    });
                    let installed = listener.map_or(false, |l| l.establish(flow, attempt, tx, rx));
                    if !installed {
                        if let Some(index) = self.flows.iter().position(|f| {
                            f.as_ref()
                                .map_or(false, |f| same(f.flow, flow) && f.attempt == attempt)
                        }) {
                            self.reset(index)
                        }
                    }
                }
            }
            if let Some(c) = self.contexts[ci].as_ref() {
                unsafe { klet_tio_host_wake(c.generation) }
            }
        }
    }
    fn reset(&mut self, index: usize) {
        let f = self.flows[index].as_mut().unwrap();
        if let Some(pipes) = f.pipes.take() {
            for p in &pipes {
                p.reset()
            }
        }
        f.terminal = [true; 2];
        let control = VsockControlMessage {
            flow: f.flow,
            attempt: f.attempt,
            kind: VsockControlKind::Reset,
        };
        let _ = self.table.control(f.flow.source.cid, control);
        for listener in self.listeners.iter_mut().flatten() {
            let matches = (listener.context == f.a
                && self.contexts[f.a]
                    .as_ref()
                    .map_or(false, |c| c.generation == f.owners[0]))
                || (listener.context == f.b
                    && self.contexts[f.b]
                        .as_ref()
                        .map_or(false, |c| c.generation == f.owners[1]));
            if matches {
                listener.reset(f.flow, f.attempt)
            }
        }
        // Withdraw not-yet-published events. Dropping Established closes its
        // unique roles; unread published results remain framework-owned.
        for (ci, c) in self
            .contexts
            .iter_mut()
            .enumerate()
            .filter_map(|(i, c)| c.as_mut().map(|c| (i, c)))
        {
            if !((ci == f.a && c.generation == f.owners[0])
                || (ci == f.b && c.generation == f.owners[1]))
            {
                continue;
            }
            for event in &mut c.events {
                let matches = event.as_ref().map_or(false, |e| match e {
                    Event::Control(m) => same(m.flow, f.flow) && m.attempt == f.attempt,
                    Event::Established { flow, attempt, .. } => {
                        same(*flow, f.flow) && *attempt == f.attempt
                    }
                });
                if matches {
                    event.take();
                }
            }
        }
    }
    fn remove(&mut self, index: usize) {
        if let Some(c) = self.contexts[index].as_mut() {
            c.removed = true;
            for r in &mut c.receive {
                if let Some(r) = r.take() {
                    r.finish::<Reply>(Err(RequestError::Removed))
                }
            }
            for e in &mut c.events {
                e.take();
            }
        }
        for i in 0..FLOWS {
            if self.flows[i].as_ref().map_or(false, |f| {
                (f.a == index
                    && f.shut[0] != 3
                    && self.contexts[index]
                        .as_ref()
                        .map_or(false, |c| c.generation == f.owners[0]))
                    || (f.b == index
                        && f.shut[1] != 3
                        && self.contexts[index]
                            .as_ref()
                            .map_or(false, |c| c.generation == f.owners[1]))
            }) {
                // A fully shut down endpoint has already published graceful
                // closure. Its later context destruction must not replace
                // that EOF with Reset while the surviving peer drains it.
                self.reset(i)
            }
        }
        for listener in &mut self.listeners {
            if listener.as_ref().map_or(false, |l| l.context == index) {
                listener.take();
            }
        }
        self.poll();
    }
    fn control(&mut self, sender: usize, message: VsockControlMessage) -> Result<(), RequestError> {
        let context = self.contexts[sender]
            .as_ref()
            .ok_or(RequestError::Removed)?;
        if context.removed || !context.flow_allowed(message.flow) {
            return Err(RequestError::Permission);
        }
        let peer = self
            .contexts
            .iter()
            .position(|c| {
                c.as_ref().map_or(false, |c| {
                    !c.removed && c.config.cid == message.flow.destination.cid
                })
            })
            .ok_or(RequestError::Invalid)?;
        let remote = self.contexts[peer].as_ref().unwrap();
        if !remote.flow_allowed(message.flow.reversed()) {
            return Err(RequestError::Permission);
        }
        let mut existing = self
            .flows
            .iter()
            .position(|f| f.as_ref().map_or(false, |f| same(f.flow, message.flow)));
        if message.kind == VsockControlKind::ConnectOffer {
            if existing.is_some() {
                return Err(RequestError::Invalid);
            }
            for side in [sender, peer] {
                let limit = self.contexts[side].as_ref().unwrap().config.max_flows as usize;
                if self
                    .flows
                    .iter()
                    .flatten()
                    .filter(|f| f.a == side || f.b == side)
                    .count()
                    >= limit
                {
                    return Err(RequestError::ResourceLimit);
                }
            }
            if remote.event_slot().is_none() {
                return Err(RequestError::ResourceLimit);
            }
            existing = self.flows.iter().position(Option::is_none);
            if existing.is_none() {
                return Err(RequestError::ResourceLimit);
            }
        }
        let index = existing.ok_or(RequestError::Invalid)?;
        if let Some(f) = self.flows[index].as_ref() {
            if self.contexts[f.a]
                .as_ref()
                .map_or(true, |c| c.generation != f.owners[0] || c.removed)
                || self.contexts[f.b]
                    .as_ref()
                    .map_or(true, |c| c.generation != f.owners[1] || c.removed)
            {
                return Err(RequestError::Removed);
            }
        }
        let action = self
            .table
            .control(context.config.cid, message)
            .map_err(|_| RequestError::Invalid)?;
        match action {
            Action::DeliverControl => {
                let c = self.contexts[peer].as_mut().unwrap();
                let slot = c.event_slot().unwrap();
                c.events[slot] = Some(Event::Control(message));
                self.flows[index] = Some(FlowData {
                    flow: message.flow,
                    attempt: message.attempt,
                    a: sender,
                    b: peer,
                    owners: [
                        self.contexts[sender].as_ref().unwrap().generation,
                        self.contexts[peer].as_ref().unwrap().generation,
                    ],
                    pipes: None,
                    terminal: [false; 2],
                    shut: [0; 2],
                });
            }
            Action::PreparePipes => {
                let flow = self.flows[index].as_ref().unwrap();
                let (a, b) = (flow.a, flow.b);
                let credit_a = self.contexts[a].as_ref().unwrap().event_slot();
                let credit_b = if a == b {
                    let context = self.contexts[b].as_ref().unwrap();
                    context
                        .events
                        .iter()
                        .enumerate()
                        .take(context.config.max_pending_events as usize)
                        .find_map(|(index, event)| {
                            (event.is_none() && credit_a != Some(index)).then_some(index)
                        })
                } else {
                    self.contexts[b].as_ref().unwrap().event_slot()
                };
                let setup = (|| {
                    let (sa, sb) = (
                        credit_a.ok_or(RequestError::ResourceLimit)?,
                        credit_b.ok_or(RequestError::ResourceLimit)?,
                    );
                    let capacity = self.contexts[a]
                        .as_ref()
                        .unwrap()
                        .config
                        .pipe_capacity
                        .min(self.contexts[b].as_ref().unwrap().config.pipe_capacity);
                    let (tx_a, rx_b, ab) = self.contexts[a]
                        .as_ref()
                        .unwrap()
                        .account
                        .create(capacity)?;
                    let (tx_b, rx_a, ba) = self.contexts[a]
                        .as_ref()
                        .unwrap()
                        .account
                        .create(capacity)?;
                    let ((tx_a, rx_a), (tx_b, rx_b)) = host_api::reserve_deliveries(
                        (tx_a, rx_a, self.contexts[a].as_ref().unwrap().generation),
                        (tx_b, rx_b, self.contexts[b].as_ref().unwrap().generation),
                    )?;
                    let f = self.flows[index].as_mut().unwrap();
                    self.table
                        .publish_established(f.flow, f.attempt)
                        .map_err(|_| RequestError::Invalid)?;
                    self.contexts[a].as_mut().unwrap().events[sa] = Some(Event::Established {
                        flow: f.flow.reversed(),
                        attempt: f.attempt,
                        tx: tx_a,
                        rx: rx_a,
                    });
                    self.contexts[b].as_mut().unwrap().events[sb] = Some(Event::Established {
                        flow: f.flow,
                        attempt: f.attempt,
                        tx: tx_b,
                        rx: rx_b,
                    });
                    f.pipes = Some([ab, ba]);
                    Ok(())
                })();
                if let Err(error) = setup {
                    let _ = self.table.preparation_failed(message.flow, message.attempt);
                    self.reset(index);
                    self.poll();
                    return Err(error);
                }
            }
            Action::CloseDirections {
                sender: side,
                directions,
            } => {
                let f = self.flows[index].as_mut().unwrap();
                let pipes = f.pipes.as_ref().ok_or(RequestError::Invalid)?;
                if matches!(
                    directions,
                    ShutdownDirection::Send | ShutdownDirection::Both
                ) {
                    pipes[side].close_producer();
                    f.shut[side] |= 1
                }
                if matches!(
                    directions,
                    ShutdownDirection::Receive | ShutdownDirection::Both
                ) {
                    pipes[1 - side].close_consumer();
                    f.shut[side] |= 2
                }
                // The pipe's terminal cause drives socket readiness even if
                // peer control slots are temporarily full.
                if let Some(slot) = self.contexts[peer].as_ref().unwrap().event_slot() {
                    self.contexts[peer].as_mut().unwrap().events[slot] =
                        Some(Event::Control(message));
                }
            }
            Action::ResetPipes => self.reset(index),
        }
        self.poll();
        Ok(())
    }
    fn handle(&mut self, index: usize, request: HostRequest<VsockRequest>) {
        self.poll();
        let Some(c) = self.contexts.get_mut(index).and_then(Option::as_mut) else {
            request.finish::<Reply>(Err(RequestError::Removed));
            return;
        };
        if c.removed || c.generation != request.caller().attachment_generation {
            request.finish::<Reply>(Err(RequestError::Permission));
            return;
        }
        let receive = matches!(request.message(), VsockRequest::ReceiveEvent);
        if !c.queue_allowed(request.queue_generation(), receive) {
            request.finish::<Reply>(Err(RequestError::Invalid));
            return;
        }
        match *request.message() {
            VsockRequest::ReceiveEvent => {
                if let Some(slot) = c.receive.iter_mut().find(|s| s.is_none()) {
                    *slot = Some(request);
                    c.drain()
                } else {
                    request.finish::<Reply>(Err(RequestError::ResourceLimit))
                }
            }
            VsockRequest::SendControl(message) => {
                let result = self.control(index, message).map(|()| Reply::ControlSent);
                request.finish(result)
            }
        }
    }
}
fn same(a: Flow, b: Flow) -> bool {
    a == b || a == b.reversed()
}
struct VsockBackend {
    state: Serialized<State>,
}
static BACKEND: VsockBackend = VsockBackend {
    state: Serialized::new(State::new()),
};
impl Backend<VsockRequest> for VsockBackend {
    fn handle(&self, r: HostRequest<VsockRequest>) {
        let generation = r.caller().attachment_generation;
        self.state.with(|s| {
            if let Some(i) = s
                .contexts
                .iter()
                .position(|c| c.as_ref().map_or(false, |c| c.generation == generation))
            {
                s.handle(i, r)
            } else {
                r.finish::<Reply>(Err(RequestError::Removed))
            }
        })
    }
}
// FFI capsule: metadata pointers are captured only on registered worker calls.
#[no_mangle]
pub unsafe extern "C" fn klet_tio_vsock_descriptor(output: *mut Descriptor) {
    unsafe { output.write(host_api::descriptor::<Protocol>()) }
}
#[no_mangle]
pub unsafe extern "C" fn klet_tio_vsock_prepare(d: *const Descriptor, generation: u64) -> i32 {
    match BACKEND.state.with(|s| s.add(unsafe { &*d }, generation)) {
        Ok(i) => (i + 1) as i32,
        Err(_) => -22,
    }
}
#[no_mangle]
pub unsafe extern "C" fn klet_tio_vsock_handle(pointer: *mut c_void) {
    match unsafe { HostRequest::<VsockRequest>::capture(pointer) } {
        Ok(r) => BACKEND.handle(r),
        Err(error) => {
            let r = Record {
                status: error as u32,
                ..Record::default()
            };
            unsafe { host_api::reject(pointer, &r) }
        }
    }
}
#[no_mangle]
pub extern "C" fn klet_tio_vsock_remove(index: usize) {
    BACKEND.state.with(|s| s.remove(index - 1));
}
#[no_mangle]
pub extern "C" fn klet_tio_vsock_release(index: usize) {
    BACKEND.state.with(|s| {
        s.remove(index - 1);
        s.contexts[index - 1] = None;
    });
}
#[no_mangle]
pub extern "C" fn klet_tio_vsock_poll() {
    BACKEND.state.with(State::poll);
}

#[no_mangle]
pub extern "C" fn klet_tio_vsock_host_listen(context: usize, port: u32, backlog: u32) -> i64 {
    BACKEND.state.with(|s| {
        let index = context.wrapping_sub(1);
        let Some(c) = s.contexts.get(index).and_then(Option::as_ref) else {
            return -19;
        };
        if !c.host || c.removed || port < c.config.port_min.0 || port > c.config.port_max.0 {
            return -13;
        }
        if s.listeners
            .iter()
            .flatten()
            .any(|l| l.context == index && l.port == port && !l.closed)
        {
            return -98;
        }
        let Some(slot) = s.listeners.iter().position(Option::is_none) else {
            return -122;
        };
        let id = s.next_listener;
        if id > i64::MAX as u64 {
            return -75;
        }
        s.next_listener += 1;
        s.listeners[slot] = Some(crate::typedio_host_socket::Listener::new(
            id,
            index,
            port,
            backlog as usize,
        ));
        id as i64
    })
}
#[no_mangle]
pub extern "C" fn klet_tio_vsock_host_ready(id: u64) -> i32 {
    BACKEND.state.with(|s| {
        s.poll();
        s.listeners
            .iter()
            .flatten()
            .find(|l| l.id == id)
            .map_or(-19, |l| l.ready() as i32)
    })
}
#[no_mangle]
pub unsafe extern "C" fn klet_tio_vsock_host_accept(id: u64, output: *mut Record) -> i32 {
    BACKEND.state.with(|s| {
        s.poll();
        let Some(listener) = s
            .listeners
            .iter_mut()
            .flatten()
            .find(|l| l.id == id && !l.closed)
        else {
            return -19;
        };
        let Some((flow, attempt, resources)) = listener.accept() else {
            return -11;
        };
        let mut record = Record::default();
        record.generation = s.contexts[listener.context].as_ref().unwrap().generation;
        record.len = message::encode(&(flow, attempt), &mut record.data).unwrap() as u32;
        record.resources[..2].copy_from_slice(&resources);
        record.resource_count = 2;
        unsafe { output.write(record) };
        0
    })
}
#[no_mangle]
pub extern "C" fn klet_tio_vsock_host_close(id: u64) {
    BACKEND.state.with(|s| {
        let Some(index) = s
            .listeners
            .iter()
            .position(|l| l.as_ref().map_or(false, |l| l.id == id))
        else {
            return;
        };
        let listener = s.listeners[index].take().unwrap();
        for (flow, attempt) in listener.pending_flows() {
            if let Some(i) = s.flows.iter().position(|f| {
                f.as_ref()
                    .map_or(false, |f| same(f.flow, flow) && f.attempt == attempt)
            }) {
                s.reset(i)
            }
        }
        s.poll();
    });
}
#[no_mangle]
pub unsafe extern "C" fn klet_tio_vsock_host_end(record: *const Record, abort: bool) {
    let record = unsafe { &*record };
    let Ok((flow, attempt)) = message::decode::<(Flow, AttemptId)>(
        <(Flow, AttemptId) as crate::Message>::SCHEMA,
        &record.data[..record.len as usize],
    ) else {
        return;
    };
    BACKEND.state.with(|s| {
        let Some(context) = s.contexts.iter().position(|c| {
            c.as_ref().map_or(false, |c| {
                c.host && !c.removed && c.generation == record.generation
            })
        }) else {
            return;
        };
        let kind = if abort {
            VsockControlKind::Reset
        } else {
            VsockControlKind::Shutdown(ShutdownDirection::Both)
        };
        let _ = s.control(
            context,
            VsockControlMessage {
                flow: flow.reversed(),
                attempt,
                kind,
            },
        );
    });
}

/// Cross-compiler schema evidence, emitted by the actual Linux Rust build.
/// Native provisioning tools must produce these same canonical bytes.
#[used]
#[no_mangle]
#[link_section = ".rodata.klet_tio_schemas"]
pub static KLET_TIO_VSOCK_SCHEMAS: [[u8; 32]; 3] = [
    <VsockConfig as crate::Message>::SCHEMA.0,
    <VsockRequest as crate::Message>::SCHEMA.0,
    <Reply as crate::Message>::SCHEMA.0,
];
