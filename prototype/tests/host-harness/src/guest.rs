// SPDX-License-Identifier: MPL-2.0
//! Test plumbing only: production guest Queue, Pipe and VM-copy sources are
//! compiled unchanged; carrier/service selection and IRQ line are mocked.
use crate::service::{Call, Descriptor};
extern "C" {
    fn native_guest_service(call: *mut Call) -> i64;
    fn native_select_guest(index: u32);
}
pub unsafe extern "C" fn service(call: *mut Call) -> i64 {
    unsafe { native_guest_service(call) }
}
#[path = "../../../vostd/src/typedio.rs"]
pub mod frontend;
use crate::queue::RequestTag;
use frontend::{discover, vsock::*, Consumer, Producer, Queue, QueueOptions};
fn select(index: u32) {
    unsafe { native_select_guest(index) }
}
fn submit(q: &Queue<VsockProtocol>, r: VsockRequest, tag: u64) {
    assert!(q.try_submit(r, RequestTag(tag)).is_ok());
}
fn pop(q: &Queue<VsockProtocol>) -> crate::queue::Completion<VsockReply> {
    for _ in 0..1000000 {
        if let Some(r) = q.try_pop_completion() {
            return r;
        }
        std::thread::yield_now()
    }
    panic!("bounded native worker completion timed out")
}
fn control(
    q: &Queue<VsockProtocol>,
    flow: Flow,
    attempt: AttemptId,
    kind: VsockControlKind,
    tag: u64,
) {
    submit(
        q,
        VsockRequest::SendControl(VsockControlMessage {
            flow,
            attempt,
            kind,
        }),
        tag,
    );
    match pop(q).result {
        Ok(VsockReply::ControlSent) => (),
        Err(e) => panic!("control tag {tag}, {kind:?}: {e:?}"),
        _ => panic!("unexpected control reply"),
    };
}
fn established(q: &Queue<VsockProtocol>) -> (Producer, Consumer) {
    static NEXT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(10000);
    loop {
        match pop(q).result {
            Ok(VsockReply::Event(VsockEvent::Established { tx, rx, .. })) => return (tx, rx),
            Ok(VsockReply::Event(VsockEvent::Control(VsockControlMessage {
                kind: VsockControlKind::Shutdown(_),
                ..
            }))) => {
                submit(
                    q,
                    VsockRequest::ReceiveEvent,
                    NEXT.fetch_add(1, core::sync::atomic::Ordering::Relaxed),
                );
            }
            Err(error) => panic!("established completion failed: {error:?}"),
            _ => panic!("missing established event"),
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn native_vsock_config(d: *mut Descriptor, cid: u64, peer: u64) {
    let mut config = VsockConfig {
        cid: ContextId(cid),
        max_flows: 4,
        pipe_capacity: 16,
        max_control_queues: 2,
        max_pending_events: 8,
        peer_cids: crate::BoundedVec::try_from([ContextId(peer)]).unwrap(),
        port_min: Port(1024),
        port_max: Port(u32::MAX),
    };
    if cid == 42 && peer == 2 {
        config.peer_cids.push(ContextId(7)).unwrap();
        config.peer_cids.push(ContextId(42)).unwrap();
    }
    let d = unsafe { &mut *d };
    *d = crate::host_api::descriptor::<
        crate::vsock::VsockProtocol<crate::host_api::Producer, crate::host_api::Consumer>,
    >();
    if cid == 2 {
        d.features = 1;
    }
    d.config_len = crate::message::encode(&config, &mut d.config).unwrap() as u32;
}
#[no_mangle]
pub extern "C" fn native_vsock_guest_roundtrip() {
    select(0);
    let a = discover::<VsockProtocol>().unwrap().pop().unwrap();
    assert_eq!(a.config().cid, ContextId(42));
    let ac = a
        .open(QueueOptions {
            depth: 8,
            features: 0,
        })
        .unwrap();
    let ar = a
        .open(QueueOptions {
            depth: 8,
            features: 0,
        })
        .unwrap();
    select(1);
    let b = discover::<VsockProtocol>().unwrap().pop().unwrap();
    let bc = b
        .open(QueueOptions {
            depth: 8,
            features: 0,
        })
        .unwrap();
    let br = b
        .open(QueueOptions {
            depth: 8,
            features: 0,
        })
        .unwrap();
    let flow = Flow {
        source: Address {
            cid: ContextId(42),
            port: Port(5000),
        },
        destination: Address {
            cid: ContextId(7),
            port: Port(8000),
        },
    };
    let attempt = AttemptId(100);
    select(1);
    submit(&br, VsockRequest::ReceiveEvent, 1);
    select(0);
    submit(&ar, VsockRequest::ReceiveEvent, 1);
    control(&ac, flow, attempt, VsockControlKind::ConnectOffer, 2);
    select(1);
    assert!(matches!(
        pop(&br).result,
        Ok(VsockReply::Event(VsockEvent::Control(
            VsockControlMessage {
                kind: VsockControlKind::ConnectOffer,
                ..
            }
        )))
    ));
    submit(&br, VsockRequest::ReceiveEvent, 3);
    control(
        &bc,
        flow.reversed(),
        attempt,
        VsockControlKind::ConnectAccept,
        4,
    );
    let (mut btx, mut brx) = established(&br);
    select(0);
    let (mut atx, mut arx) = established(&ar);
    let line = crate::irq::IrqLine;
    atx.bind_irq(&line).unwrap();
    arx.bind_irq(&line).unwrap();
    atx.set_interest(frontend::ProducerInterest::SPACE_AVAILABLE)
        .unwrap();
    arx.set_interest(frontend::ConsumerInterest::DATA_AVAILABLE)
        .unwrap();
    atx.take_events();
    arx.take_events();
    assert!(atx.rearm_irq());
    assert!(arx.rearm_irq());
    // Real reservation-scoped VM copies; both directions use identical generic
    // storage, no echo-specific data transport or backing exists.
    let input = b"hello typedio";
    let mut reader = crate::mm::VmReader::from(&input[..]).to_fallible();
    let mut r = atx.try_reserve(99).unwrap();
    assert_eq!(r.produce(&mut reader), Ok(input.len()));
    r.commit(input.len()).unwrap();
    select(1);
    let mut output = [0u8; 32];
    let mut dst = crate::mm::VmWriter::from(&mut output[..3]).to_fallible();
    let mut r = brx.try_reserve(3).unwrap().unwrap();
    assert_eq!(r.consume(&mut dst), Ok(3));
    r.commit(0).unwrap();
    assert_eq!(&output[..3], b"hel");
    let mut dst = crate::mm::VmWriter::from(&mut output[..input.len()]).to_fallible();
    let mut r = brx.try_reserve(99).unwrap().unwrap();
    assert_eq!(r.consume(&mut dst), Ok(input.len()));
    r.commit(input.len()).unwrap();
    assert_eq!(&output[..input.len()], input);
    let mut reader = crate::mm::VmReader::from(&output[..input.len()]).to_fallible();
    let mut r = btx.try_reserve(99).unwrap();
    let n = r.produce(&mut reader).unwrap();
    r.commit(n).unwrap();
    select(0);
    let mut echoed = [0u8; 32];
    let mut dst = crate::mm::VmWriter::from(&mut echoed[..n]).to_fallible();
    let mut r = arx.try_reserve(99).unwrap().unwrap();
    assert_eq!(r.consume(&mut dst), Ok(n));
    r.commit(n).unwrap();
    assert_eq!(&echoed[..n], input);
    extern "C" {
        fn native_drain_irq_sources() -> i32;
    }
    assert!(unsafe { native_drain_irq_sources() } >= 2);
    assert!(matches!(
        arx.try_reserve(1),
        Err(frontend::PipeError::WouldBlock)
    ));
    let r = atx.try_reserve(16).unwrap();
    assert!(r.commit(1).is_err()); // commit cannot exceed recorded progress
                                   // Half close preserves already published data and exposes EOF.
    control(
        &ac,
        flow,
        attempt,
        VsockControlKind::Shutdown(ShutdownDirection::Send),
        5,
    );
    select(1);
    assert!(brx.try_reserve(1).unwrap().is_none());
    // Cancellation retires an independently retained receive without a worker
    // waiting for any guest byte or event.
    let id = br
        .try_submit(VsockRequest::ReceiveEvent, RequestTag(8))
        .ok()
        .unwrap();
    br.cancel(id).unwrap();
    // A queued Shutdown may consume this slot first. Post/cancel another when
    // that happens, and assert its eventual final cancellation.
    let completion = pop(&br);
    if completion.result.is_ok() {
        let id = br
            .try_submit(VsockRequest::ReceiveEvent, RequestTag(9))
            .ok()
            .unwrap();
        br.cancel(id).unwrap();
        assert!(pop(&br).result.is_err());
    } else {
        assert_eq!(
            completion.result.err(),
            Some(crate::queue::RequestError::Cancelled)
        );
    }
    drop(btx);
    drop(brx);
    drop(bc);
    drop(br);
    select(0);
    drop(atx);
    drop(arx);
    drop(ac);
    drop(ar);
}

#[no_mangle]
pub extern "C" fn native_vsock_host_roundtrip(listener: i32) {
    extern "C" {
        fn native_fail_next_fd();
        fn native_start_blocking_read(fd: i32);
        fn native_interrupt_blocking_read();
        fn native_wait_for_blocking_read();
        fn native_join_blocking_read(buffer: *mut u8, len: usize) -> isize;
        fn native_set_copy_fault(bytes: usize);
        fn native_host_accept(fd: i32) -> i32;
        fn native_host_read(fd: i32, buffer: *mut u8, len: usize) -> isize;
        fn native_host_write(fd: i32, buffer: *const u8, len: usize) -> isize;
        fn native_host_poll(fd: i32) -> u32;
        fn native_close(fd: i32);
    }
    select(0);
    let device = discover::<VsockProtocol>().unwrap().pop().unwrap();
    let control_queue = device
        .open(QueueOptions {
            depth: 8,
            features: 0,
        })
        .unwrap();
    let receive = device
        .open(QueueOptions {
            depth: 8,
            features: 0,
        })
        .unwrap();
    let flow = Flow {
        source: Address {
            cid: ContextId(42),
            port: Port(5001),
        },
        destination: Address {
            cid: ContextId(2),
            port: Port(8000),
        },
    };
    let attempt = AttemptId(101);
    submit(&receive, VsockRequest::ReceiveEvent, 1);
    control(
        &control_queue,
        flow,
        attempt,
        VsockControlKind::ConnectOffer,
        2,
    );
    let (mut tx, mut rx) = established(&receive);
    assert_ne!(unsafe { native_host_poll(listener) } & 1, 0);
    let fd = unsafe { native_host_accept(listener) };
    assert!(fd >= 3);
    assert_ne!(unsafe { native_host_poll(fd) } & 4, 0); // POLLOUT
    let mut buffer = [0u8; 32];
    assert_eq!(
        unsafe { native_host_read(fd, buffer.as_mut_ptr(), 32) },
        -11
    );
    let input = b"hostecho";
    unsafe {
        native_start_blocking_read(fd);
        native_interrupt_blocking_read();
    };
    assert_eq!(
        unsafe { native_join_blocking_read(buffer.as_mut_ptr(), buffer.len()) },
        -512
    );
    unsafe {
        native_start_blocking_read(fd);
        native_wait_for_blocking_read();
    };
    assert_eq!(unsafe { native_host_poll(fd) } & 1, 0);
    let mut first = crate::mm::VmReader::from(&input[..]).to_fallible();
    let mut first_reservation = tx.try_reserve(input.len()).unwrap();
    let first_n = first_reservation.produce(&mut first).unwrap();
    first_reservation.commit(first_n).unwrap();
    assert_eq!(
        unsafe { native_join_blocking_read(buffer.as_mut_ptr(), buffer.len()) },
        input.len() as isize
    );
    assert_eq!(&buffer[..input.len()], input);
    let mut reader = crate::mm::VmReader::from(&input[..]).to_fallible();
    let mut reservation = tx.try_reserve(input.len()).unwrap();
    let n = reservation.produce(&mut reader).unwrap();
    reservation.commit(n).unwrap();
    assert_ne!(unsafe { native_host_poll(fd) } & 1, 0);
    unsafe { native_set_copy_fault(3) };
    assert_eq!(unsafe { native_host_read(fd, buffer.as_mut_ptr(), 32) }, 3);
    assert_eq!(&buffer[..3], &input[..3]);
    let rest = unsafe { native_host_read(fd, buffer.as_mut_ptr().add(3), 29) };
    assert_eq!(rest as usize, input.len() - 3);
    assert_eq!(&buffer[..input.len()], input);
    unsafe { native_set_copy_fault(4) };
    assert_eq!(
        unsafe { native_host_write(fd, buffer.as_ptr(), input.len()) },
        4
    );
    assert_eq!(
        unsafe { native_host_write(fd, buffer.as_ptr().add(4), input.len() - 4) },
        (input.len() - 4) as isize
    );
    let mut output = [0u8; 32];
    let mut dst = crate::mm::VmWriter::from(&mut output[..input.len()]).to_fallible();
    let mut r = rx.try_reserve(32).unwrap().unwrap();
    assert_eq!(r.consume(&mut dst), Ok(input.len()));
    r.commit(input.len()).unwrap();
    assert_eq!(&output[..input.len()], input);
    unsafe { native_close(fd) };
    assert!(rx.try_reserve(1).unwrap().is_none());
    drop(tx);
    drop(rx);
    // Descriptor exposure failure must Reset the already established flow and
    // release both host cleanup receipts rather than silently orphaning pipes.
    let failed_flow = Flow {
        source: Address {
            cid: ContextId(42),
            port: Port(5010),
        },
        destination: flow.destination,
    };
    submit(&receive, VsockRequest::ReceiveEvent, 30);
    control(
        &control_queue,
        failed_flow,
        AttemptId(102),
        VsockControlKind::ConnectOffer,
        31,
    );
    let (mut failed_tx, mut failed_rx) = established(&receive);
    unsafe { native_fail_next_fd() };
    assert_eq!(unsafe { native_host_accept(listener) }, -24);
    assert!(matches!(
        failed_tx.try_reserve(1),
        Err(frontend::PipeError::Reset)
    ));
    assert!(matches!(
        failed_rx.try_reserve(1),
        Err(frontend::PipeError::Reset)
    ));
    drop(failed_tx);
    drop(failed_rx);
    drop(control_queue);
    drop(receive);
}
