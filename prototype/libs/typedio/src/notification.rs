// SPDX-License-Identifier: MPL-2.0

//! One source's publication/rearm state. Callers hold that source's host lock
//! across every operation, including publication and the empty/rearm decision.
//! The vCPU interrupt bit is only a prompt; this record is authoritative.

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// A host route with a fixed vCPU and non-reused line generation.
pub struct Binding {
    /// The local line slot.
    pub line: u32,
    /// The fixed destination virtual CPU.
    pub vcpu: u32,
    /// The generation that must match before routing.
    pub generation: u64,
}

#[derive(Debug)]
/// Latched causes and delivery suppression for one lock-protected source.
pub struct Notification {
    pending: u8,
    interest: u8,
    terminal: u8,
    enabled: bool,
    binding: Option<Binding>,
    delivery_requested: bool,
}
impl Notification {
    /// Creates an unbound source with the given terminal-cause mask.
    pub const fn new(terminal: u8) -> Self {
        Self {
            pending: 0,
            interest: 0,
            terminal,
            enabled: false,
            binding: None,
            delivery_requested: false,
        }
    }
    /// Installs routing with delivery suppressed until explicit rearm.
    pub fn bind(&mut self, binding: Binding) {
        self.binding = Some(binding);
        self.enabled = false;
        self.delivery_requested = false;
    }
    /// Invalidate routing before a line identity can be reused.
    pub fn unbind(&mut self) {
        self.binding = None;
        self.enabled = false;
        self.delivery_requested = false;
    }
    /// Returns the current host route.
    pub fn binding(&self) -> Option<Binding> {
        self.binding
    }
    /// Returns latched causes without consuming them.
    pub fn pending(&self) -> u8 {
        self.pending
    }
    /// Replaces readiness interest and captures newly enabled ready levels.
    pub fn set_interest(&mut self, interest: u8, current_ready: u8, current_terminal: u8) {
        let interest = interest & !self.terminal;
        let newly_enabled = interest & !self.interest;
        self.pending &= self.terminal | interest;
        self.pending |= (newly_enabled & current_ready) | (current_terminal & self.terminal);
        self.interest = interest;
        self.enabled = false;
    }
    /// Returns a route only on the enabled-to-suppressed transition. The host
    /// must retain the source identity independently of its coalesced IRQ bit.
    pub fn publish(&mut self, causes: u8) -> Option<Binding> {
        self.pending |= causes & (self.interest | self.terminal);
        if self.pending != 0 && self.enabled {
            self.enabled = false;
            self.delivery_requested = self.binding.is_some();
            self.binding
        } else {
            None
        }
    }
    /// Takes the retained routing request after a metadata transition.
    pub fn take_delivery(&mut self) -> Option<Binding> {
        if self.delivery_requested {
            self.delivery_requested = false;
            self.binding
        } else {
            None
        }
    }
    /// Atomically takes causes under the source publication lock.
    pub fn take_events(&mut self) -> u8 {
        let pending = self.pending;
        self.pending = 0;
        pending
    }
    /// `records_pending` includes unread queue completions. For pipes it is
    /// false: an already-ready level is not a new cause and must not IRQ-loop.
    pub fn rearm(&mut self, records_pending: bool) -> bool {
        if self.pending != 0 || records_pending {
            self.enabled = false;
            false
        } else {
            self.enabled = true;
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> Notification {
        let mut n = Notification::new(4);
        n.bind(Binding {
            line: 1,
            vcpu: 0,
            generation: 1,
        });
        n.set_interest(1, 0, 0);
        n
    }
    #[test]
    fn publish_during_drain_and_empty_rearm_gap() {
        let mut n = source();
        assert!(n.rearm(false));
        assert!(n.publish(1).is_some());
        assert_eq!(n.take_events(), 1);
        // Publication after the frontend observed an empty queue.
        assert!(n.publish(1).is_none());
        assert!(!n.rearm(false));
        assert_eq!(n.take_events(), 1);
        assert!(n.rearm(false));
        assert!(n.publish(1).is_some());
    }
    #[test]
    fn interest_install_checks_level_but_rearm_does_not() {
        let mut n = source();
        n.set_interest(0, 1, 0);
        assert!(n.rearm(false));
        assert!(n.publish(1).is_none());
        n.set_interest(1, 1, 0);
        assert!(!n.rearm(false));
        assert_eq!(n.take_events(), 1);
        assert!(n.rearm(false));
        assert!(n.rearm(false));
    }
    #[test]
    fn terminal_survives_interest_removal_and_unbind() {
        let mut n = source();
        n.publish(5);
        n.set_interest(0, 0, 4);
        assert_eq!(n.take_events(), 4);
        n.unbind();
        assert!(n.rearm(false));
        assert!(n.publish(4).is_none());
        assert_eq!(n.take_events(), 4);
    }
    #[test]
    fn shared_line_keeps_sources_independent() {
        let mut a = source();
        let mut b = source();
        assert!(a.rearm(false));
        assert!(b.rearm(false));
        assert_eq!(a.publish(1), b.publish(1));
        assert_eq!(a.take_events(), 1);
        assert_eq!(b.pending(), 1);
        assert!(!b.rearm(false));
    }
}
