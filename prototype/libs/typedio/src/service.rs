// SPDX-License-Identifier: MPL-2.0
//! Versioned scalar/metadata records for the trusted host service. No Rust
//! owner, destructor, callback, or reference representation crosses this ABI.

pub const ABI_VERSION: u32 = 2;
pub const MAX_METADATA: usize = 128;
pub const MAX_QUEUE_DEPTH: usize = 8;
pub const MAX_ATTACHMENTS: usize = 8;
pub const DISCOVER: u32 = 1;
pub const OPEN: u32 = 2;
pub const SUBMIT: u32 = 3;
pub const POP: u32 = 4;
pub const CANCEL: u32 = 5;
pub const EVENTS: u32 = 6;
pub const REARM: u32 = 7;
pub const CLOSE: u32 = 8;
pub const BIND: u32 = 9;
pub const IRQ_POLL: u32 = 10;
pub const UNBIND_LINE: u32 = 11;

// Audited capability tickets are interpreted only under framework ownership.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Resource {
    pub slot: u64,
    pub generation: u64,
    pub owner: u64,
    pub kind: u32,
    pub reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Descriptor {
    pub abi_version: u32,
    pub protocol_version: u32,
    pub features: u64,
    pub protocol: [u8; 16],
    pub schemas: [[u8; 32]; 3],
    pub config_len: u32,
    pub reserved: u32,
    pub config: [u8; MAX_METADATA],
}
impl Default for Descriptor {
    fn default() -> Self {
        Self {
            abi_version: ABI_VERSION,
            protocol_version: 1,
            features: 0,
            protocol: [0; 16],
            schemas: [[0; 32]; 3],
            config_len: 0,
            reserved: 0,
            config: [0; MAX_METADATA],
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record {
    pub generation: u64,
    pub sequence: u64,
    pub tag: u64,
    pub len: u32,
    pub status: u32,
    pub data: [u8; MAX_METADATA],
    pub resources: [Resource; 4],
    pub resource_count: u32,
    pub reserved: u32,
}
impl Default for Record {
    fn default() -> Self {
        Self {
            generation: 0,
            sequence: 0,
            tag: 0,
            len: 0,
            status: 0,
            data: [0; MAX_METADATA],
            resources: [Resource::default(); 4],
            resource_count: 0,
            reserved: 0,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Call {
    pub operation: u32,
    pub flags: u32,
    pub handle: u64,
    pub generation: u64,
    pub arg0: u64,
    pub arg1: u64,
    pub record: Record,
    pub descriptor: Descriptor,
}

pub const PIPE_RESERVE: u32 = 16;
pub const PIPE_PROGRESS: u32 = 17;
pub const PIPE_COMMIT: u32 = 18;
pub const PIPE_CANCEL: u32 = 19;
pub const PIPE_DROP: u32 = 20;
