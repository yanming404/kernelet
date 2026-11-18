// SPDX-License-Identifier: MPL-2.0
//! Host control-plane provisioning only. No payload forwarding or backend runs
//! in this process: the inherited provider/listener FD delegates kernel authority.
use std::{
    ffi::{c_int, c_ulong},
    fs::OpenOptions,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::process::CommandExt,
    },
    process::Command,
};
use typedio::{
    service::Descriptor,
    vsock::{ContextId, Port, VsockConfig, VsockProtocol},
    Message, Protocol, SchemaDigest,
};
extern "C" {
    fn ioctl(fd: c_int, command: c_ulong, ...) -> c_int;
    fn fcntl(fd: c_int, command: c_int, ...) -> c_int;
}
struct ProducerSchema;
struct ConsumerSchema;
// SAFETY: schema-only leaves cannot encode/decode or construct a capability.
// Their constants come from the same audited definitions as operational leaves.
unsafe impl Message for ProducerSchema {
    const SCHEMA: SchemaDigest = typedio::message::PIPE_PRODUCER_SCHEMA;
}
unsafe impl Message for ConsumerSchema {
    const SCHEMA: SchemaDigest = typedio::message::PIPE_CONSUMER_SCHEMA;
}
type ProtocolType = VsockProtocol<ProducerSchema, ConsumerSchema>;
#[repr(C)]
struct Listen {
    port: u32,
    backlog: u32,
    flags: u32,
    reserved: u32,
}
fn command(size: usize, kind: u8, number: u8) -> c_ulong {
    (1 << 30) | ((size as c_ulong) << 16) | ((kind as c_ulong) << 8) | number as c_ulong
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|value| value == "--schema-bytes") {
        use std::io::Write;
        for digest in [
            VsockConfig::SCHEMA,
            <ProtocolType as Protocol>::Request::SCHEMA,
            <ProtocolType as Protocol>::Reply::SCHEMA,
        ] {
            std::io::stdout().write_all(&digest.0)?;
        }
        return Ok(());
    }
    if args.first().is_none_or(|v| v == "--help") {
        println!("usage: typedio-device CID PEER[,PEER...] [--listen PORT] -- PROGRAM [ARG...]\nCreates a privileged kernel provider; guest FDs use KLET_TYPEDIO_FDS, host listeners use KLET_TYPEDIO_LISTENER_FD. No module is loaded and no benchmark runs.");
        return Ok(());
    }
    let split = args
        .iter()
        .position(|a| a == "--")
        .ok_or("missing -- PROGRAM")?;
    if split < 2 || args.len() <= split + 1 {
        return Err("CID, peers and a command are required".into());
    }
    let cid: u64 = args[0].parse()?;
    if cid < 2 {
        return Err("CID must be 2 (host) or at least 3 (guest)".into());
    }
    let mut peers = typedio::BoundedVec::<ContextId, 8>::new();
    for peer in args[1].split(',') {
        peers
            .push(ContextId(peer.parse()?))
            .map_err(|_| "at most eight peer CIDs are supported")?;
    }
    let config = VsockConfig {
        cid: ContextId(cid),
        max_flows: 16,
        pipe_capacity: 65536,
        max_control_queues: 2,
        max_pending_events: 16,
        peer_cids: peers,
        port_min: Port(1024),
        port_max: Port(u32::MAX - 1),
    };
    let mut descriptor = Descriptor {
        protocol: ProtocolType::ID.0,
        schemas: [
            VsockConfig::SCHEMA.0,
            <ProtocolType as Protocol>::Request::SCHEMA.0,
            <ProtocolType as Protocol>::Reply::SCHEMA.0,
        ],
        features: if cid == 2 { 1 } else { 0 },
        ..Descriptor::default()
    };
    descriptor.config_len = typedio::message::encode(&config, &mut descriptor.config)
        .map_err(|_| "configuration exceeds metadata capacity")? as u32;
    let system = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/endovisor")?;
    let fd = unsafe {
        ioctl(
            system.as_raw_fd(),
            command(std::mem::size_of::<Descriptor>(), 0xeb, 5),
            &descriptor,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let provider = unsafe { OwnedFd::from_raw_fd(fd) };
    let listener = if split == 4 && args[2] == "--listen" && cid == 2 {
        let listen = Listen {
            port: args[3].parse()?,
            backlog: 16,
            flags: 0x80000,
            reserved: 0,
        };
        let fd = unsafe {
            ioctl(
                provider.as_raw_fd(),
                command(std::mem::size_of::<Listen>(), b'T', 1),
                &listen,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Some(unsafe { OwnedFd::from_raw_fd(fd) })
    } else {
        if split != 2 {
            return Err("only host contexts accept --listen PORT".into());
        }
        None
    };
    let inherited = listener.as_ref().unwrap_or(&provider).as_raw_fd();
    let flags = unsafe { fcntl(inherited, 1) };
    if flags < 0 || unsafe { fcntl(inherited, 2, flags & !1) } < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut child = Command::new(&args[split + 1]);
    child.args(&args[split + 2..]);
    if listener.is_some() {
        child.env("KLET_TYPEDIO_LISTENER_FD", inherited.to_string());
    } else if cid == 2 {
        child.env("KLET_TYPEDIO_PROVIDER_FD", inherited.to_string());
    } else {
        let old = std::env::var("KLET_TYPEDIO_FDS").unwrap_or_default();
        let fds = if old.is_empty() {
            inherited.to_string()
        } else {
            format!("{old},{inherited}")
        };
        child.env("KLET_TYPEDIO_FDS", fds);
    }
    Err(child.exec().into())
}
