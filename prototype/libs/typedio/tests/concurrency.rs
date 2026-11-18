// SPDX-License-Identifier: MPL-2.0
//! Native concurrency tests of the actual shared metadata engine, not hardware IRQ tests.
use std::sync::{Arc, Barrier, Mutex};
use typedio::{
    irq::{PendingSource, Router},
    notification::{Binding, Notification},
};

#[test]
fn publication_racing_interest_installation_is_always_discoverable() {
    for _ in 0..128 {
        let mut source = Notification::new(4);
        source.bind(Binding {
            line: 0,
            vcpu: 0,
            generation: 1,
        });
        let state = Arc::new(Mutex::new((source, false)));
        let start = Arc::new(Barrier::new(3));
        let publication = {
            let state = state.clone();
            let start = start.clone();
            std::thread::spawn(move || {
                start.wait();
                let mut state = state.lock().unwrap();
                state.1 = true;
                state.0.publish(1);
            })
        };
        let interest = {
            let state = state.clone();
            let start = start.clone();
            std::thread::spawn(move || {
                start.wait();
                let mut state = state.lock().unwrap();
                let ready = if state.1 { 1 } else { 0 };
                state.0.set_interest(1, ready, 0);
            })
        };
        start.wait();
        publication.join().unwrap();
        interest.join().unwrap();
        let mut state = state.lock().unwrap();
        assert!(!state.0.rearm(false));
        assert_eq!(state.0.take_events(), 1);
        assert!(state.0.rearm(false));
    }
}

#[test]
fn detach_racing_irq_publication_never_leaves_a_dead_instance_route() {
    for _ in 0..128 {
        let mut router = Router::<1, 1>::new();
        let line = router.alloc_line(42, 0).unwrap();
        let source = router
            .bind(
                line,
                PendingSource {
                    instance: 42,
                    resource: 7,
                    resource_generation: 1,
                },
            )
            .unwrap();
        let router = Arc::new(Mutex::new(router));
        let start = Arc::new(Barrier::new(3));
        let publication = {
            let router = router.clone();
            let start = start.clone();
            std::thread::spawn(move || {
                start.wait();
                let _ = router.lock().unwrap().mark_pending(source);
            })
        };
        let death = {
            let router = router.clone();
            let start = start.clone();
            std::thread::spawn(move || {
                start.wait();
                router.lock().unwrap().detach_instance(42);
            })
        };
        start.wait();
        publication.join().unwrap();
        death.join().unwrap();
        assert!(router.lock().unwrap().take_pending(line).is_err());
    }
}
