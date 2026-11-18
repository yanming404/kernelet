// SPDX-License-Identifier: MPL-2.0

//! Host routing records, independent of the vCPU summary interrupt bit.
//! The surrounding framework serializes these operations with binding teardown.
//! Local callback addresses never appear in these records.

use crate::notification::Binding;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Permission,
    Stale,
    Quota,
    Busy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceId {
    slot: usize,
    generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingSource {
    pub instance: u64,
    pub resource: u64,
    pub resource_generation: u64,
}
#[derive(Clone, Copy)]
struct Line {
    binding: Binding,
    instance: u64,
    live: bool,
    next_source: usize,
}
#[derive(Clone, Copy)]
struct Source {
    generation: u64,
    record: Option<SourceRecord>,
}
#[derive(Clone, Copy)]
struct SourceRecord {
    binding: Binding,
    identity: PendingSource,
    pending: bool,
}

pub struct Router<const L: usize, const S: usize> {
    lines: [Line; L],
    sources: [Source; S],
}
impl<const L: usize, const S: usize> Default for Router<L, S> {
    fn default() -> Self {
        Self::new()
    }
}
impl<const L: usize, const S: usize> Router<L, S> {
    pub fn new() -> Self {
        Self {
            lines: core::array::from_fn(|i| Line {
                binding: Binding {
                    line: i as u32,
                    vcpu: 0,
                    generation: 0,
                },
                instance: 0,
                live: false,
                next_source: 0,
            }),
            sources: [Source {
                generation: 0,
                record: None,
            }; S],
        }
    }
    pub fn alloc_line(&mut self, instance: u64, vcpu: u32) -> Result<Binding, Error> {
        if instance == 0 {
            return Err(Error::Invalid);
        }
        let line = self
            .lines
            .iter_mut()
            .find(|x| !x.live && x.binding.generation != u64::MAX)
            .ok_or(Error::Quota)?;
        line.binding.generation += 1;
        line.binding.vcpu = vcpu;
        line.instance = instance;
        line.live = true;
        Ok(line.binding)
    }
    fn validate(&self, binding: Binding) -> Result<(), Error> {
        let line = self
            .lines
            .get(binding.line as usize)
            .ok_or(Error::Invalid)?;
        if !line.live || line.binding != binding {
            return Err(Error::Stale);
        }
        Ok(())
    }
    pub fn bind(&mut self, binding: Binding, identity: PendingSource) -> Result<SourceId, Error> {
        self.validate(binding)?;
        if self.lines[binding.line as usize].instance != identity.instance {
            return Err(Error::Permission);
        }
        if identity.resource_generation == 0 {
            return Err(Error::Invalid);
        }
        if self
            .sources
            .iter()
            .filter_map(|x| x.record.as_ref())
            .any(|x| x.identity == identity)
        {
            return Err(Error::Busy);
        }
        let slot = self
            .sources
            .iter()
            .position(|x| x.record.is_none() && x.generation != u64::MAX)
            .ok_or(Error::Quota)?;
        let entry = &mut self.sources[slot];
        entry.generation += 1;
        entry.record = Some(SourceRecord {
            binding,
            identity,
            pending: false,
        });
        Ok(SourceId {
            slot,
            generation: entry.generation,
        })
    }
    fn source(&mut self, id: SourceId) -> Result<&mut SourceRecord, Error> {
        let entry = self.sources.get_mut(id.slot).ok_or(Error::Invalid)?;
        if entry.generation != id.generation {
            return Err(Error::Stale);
        }
        entry.record.as_mut().ok_or(Error::Stale)
    }
    pub fn mark_pending(&mut self, id: SourceId) -> Result<Binding, Error> {
        let binding = self.source(id)?.binding;
        self.validate(binding)?;
        self.source(id)?.pending = true;
        Ok(binding)
    }
    /// Acknowledges a prompt, not a queue completion or a pipe event. Work
    /// published after this acknowledgement leaves another pending record.
    pub fn take_pending(&mut self, binding: Binding) -> Result<Option<PendingSource>, Error> {
        self.validate(binding)?;
        let start = self.lines[binding.line as usize].next_source;
        for offset in 0..S {
            let slot = (start + offset) % S;
            if let Some(source) = &mut self.sources[slot].record {
                if source.binding == binding && source.pending {
                    source.pending = false;
                    self.lines[binding.line as usize].next_source = (slot + 1) % S;
                    return Ok(Some(source.identity));
                }
            }
        }
        Ok(None)
    }
    pub fn unbind(&mut self, id: SourceId) -> Result<(), Error> {
        self.source(id)?;
        self.sources[id.slot].record = None;
        Ok(())
    }
    pub fn free_line(&mut self, binding: Binding) -> Result<(), Error> {
        self.validate(binding)?;
        // Withdraw every route before this line slot can acquire a new
        // generation. Pending causes remain in each queue/pipe source itself.
        for entry in &mut self.sources {
            if entry.record.as_ref().is_some_and(|x| x.binding == binding) {
                entry.record = None;
            }
        }
        self.lines[binding.line as usize].live = false;
        Ok(())
    }
    pub fn detach_instance(&mut self, instance: u64) {
        for line in &mut self.lines {
            if line.instance == instance {
                line.live = false;
            }
        }
        for entry in &mut self.sources {
            if entry
                .record
                .as_ref()
                .is_some_and(|x| x.identity.instance == instance)
            {
                entry.record = None;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn identity(resource: u64) -> PendingSource {
        PendingSource {
            instance: 1,
            resource,
            resource_generation: 1,
        }
    }
    #[test]
    fn coalesced_irq_does_not_erase_independent_pending_sources() {
        let mut r = Router::<1, 2>::new();
        let line = r.alloc_line(1, 0).unwrap();
        let a = r.bind(line, identity(10)).unwrap();
        let b = r.bind(line, identity(11)).unwrap();
        r.mark_pending(a).unwrap();
        r.mark_pending(b).unwrap();
        assert_eq!(r.take_pending(line).unwrap(), Some(identity(10)));
        r.mark_pending(a).unwrap();
        assert_eq!(r.take_pending(line).unwrap(), Some(identity(11)));
        assert_eq!(r.take_pending(line).unwrap(), Some(identity(10)));
        assert_eq!(r.take_pending(line).unwrap(), None);
    }
    #[test]
    fn stale_binding_and_source_cannot_reach_replacement() {
        let mut r = Router::<1, 1>::new();
        let line = r.alloc_line(1, 0).unwrap();
        let a = r.bind(line, identity(10)).unwrap();
        r.mark_pending(a).unwrap();
        r.free_line(line).unwrap();
        let next = r.alloc_line(1, 1).unwrap();
        assert_ne!(line.generation, next.generation);
        let b = r.bind(next, identity(11)).unwrap();
        assert_eq!(r.mark_pending(a), Err(Error::Stale));
        assert_eq!(r.take_pending(line), Err(Error::Stale));
        assert_eq!(r.take_pending(next).unwrap(), None);
        assert_eq!(r.mark_pending(b), Ok(next));
    }
    #[test]
    fn instance_detachment_removes_routes_without_guest_callbacks() {
        let mut r = Router::<1, 1>::new();
        let line = r.alloc_line(1, 0).unwrap();
        let a = r.bind(line, identity(10)).unwrap();
        r.mark_pending(a).unwrap();
        r.detach_instance(1);
        assert_eq!(r.mark_pending(a), Err(Error::Stale));
        assert_eq!(r.take_pending(line), Err(Error::Stale));
        let replacement = r.alloc_line(2, 0).unwrap();
        assert_ne!(replacement.generation, line.generation);
    }
}
