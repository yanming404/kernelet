// SPDX-License-Identifier: MPL-2.0
use super::typedio::*;
use crate::{
    mm::{FallibleVmRead, VmReader, VmWriter},
    net::socket::{
        util::{RecvFlags, SendFlags, SockShutdownCmd},
        vsock::addr::VsockSocketAddr,
    },
    prelude::*,
    process::signal::Pollee,
    util::{MultiRead, MultiWrite},
};
unsafe extern "C" {
    fn native_host_accept(fd: i32) -> i32;
    fn native_host_read(fd: i32, buffer: *mut u8, len: usize) -> isize;
    fn native_host_write(fd: i32, buffer: *const u8, len: usize) -> isize;
    fn native_host_poll(fd: i32) -> u32;
    fn native_close(fd: i32);
}
struct Source<'a>(&'a [u8]);
impl MultiRead for Source<'_> {
    fn sum_lens(&self) -> usize {
        self.0.len()
    }
    fn read(
        &mut self,
        dst: &mut VmWriter<'_, crate::mm::Infallible>,
    ) -> Result<usize, (crate::Error, usize)> {
        let mut r = VmReader::from(self.0).to_fallible();
        let result = r.read_fallible(dst);
        let n = match result {
            Ok(n) | Err((_, n)) => n,
        };
        self.0 = &self.0[n..];
        result
    }
}
struct Destination<'a>(&'a mut [u8]);
impl MultiWrite for Destination<'_> {
    fn sum_lens(&self) -> usize {
        self.0.len()
    }
    fn write(
        &mut self,
        src: &mut VmReader<'_, crate::mm::Infallible>,
    ) -> Result<usize, (crate::Error, usize)> {
        let mut w = VmWriter::from(&mut *self.0).to_fallible();
        src.read_fallible(&mut w)
    }
}
fn wait<T>(mut run: impl FnMut() -> Option<T>) -> T {
    for _ in 0..1000000 {
        if let Some(v) = run() {
            return v;
        }
        std::thread::yield_now()
    }
    panic!("native Asterinas worker timed out")
}
fn connect(local: u32, peer: u32, port: u32) -> Connection {
    let p = BoundPort::new_exclusive(VsockSocketAddr {
        cid: 42,
        port: local,
    })
    .unwrap();
    let mut c = p
        .connect(VsockSocketAddr { cid: peer, port }, &Pollee::new())
        .ok()
        .unwrap();
    wait(|| c.has_connect_result().then_some(()));
    loop {
        match c.finish_connect() {
            ConnectResult::Connected(c) => return c,
            ConnectResult::Connecting(next) => {
                c = next;
                std::thread::yield_now()
            }
            ConnectResult::Failed(_, e) => panic!("connect failed: {e:?}"),
        }
    }
}
pub(super) fn run(listener: i32) {
    init();
    let mut c = connect(5002, 2, 8000);
    wait(|| (unsafe { native_host_poll(listener) } & 1 != 0).then_some(()));
    let fd = unsafe { native_host_accept(listener) };
    assert!(fd >= 3);
    let input = b"socket echo";
    assert_eq!(
        wait(|| match c.try_send(&mut Source(input), SendFlags) {
            Ok(n) => Some(n),
            Err(e) if e.errno == Errno::EAGAIN => None,
            Err(e) => panic!("send: {e:?}"),
        }),
        input.len()
    );
    let mut buffer = [0u8; 32];
    let n = wait(|| {
        let n = unsafe { native_host_read(fd, buffer.as_mut_ptr(), 32) };
        (n >= 0).then_some(n)
    });
    assert_eq!(n as usize, input.len());
    assert_eq!(&buffer[..input.len()], input);
    assert_eq!(
        unsafe { native_host_write(fd, buffer.as_ptr(), input.len()) },
        input.len() as isize
    );
    wait(|| {
        c.check_io_events()
            .contains(crate::events::IoEvents::IN)
            .then_some(())
    });
    let mut peek = [0u8; 3];
    assert_eq!(
        c.try_recv(&mut Destination(&mut peek), RecvFlags(true))
            .unwrap(),
        3
    );
    assert_eq!(&peek, &input[..3]);
    let mut output = [0u8; 32];
    assert_eq!(
        c.try_recv(&mut Destination(&mut output), RecvFlags(false))
            .unwrap(),
        input.len()
    );
    assert_eq!(&output[..input.len()], input);
    c.shutdown(SockShutdownCmd::SHUT_WR).unwrap();
    assert_eq!(
        wait(|| {
            let n = unsafe { native_host_read(fd, buffer.as_mut_ptr(), 32) };
            (n >= 0).then_some(n)
        }),
        0
    );
    unsafe { native_close(fd) };
    wait(|| {
        c.check_io_events()
            .contains(crate::events::IoEvents::RDHUP)
            .then_some(())
    });
    assert_eq!(
        c.try_recv(&mut Destination(&mut output), RecvFlags(false))
            .unwrap(),
        0
    );
    drop(c);
    let listener = BoundPort::new_exclusive(VsockSocketAddr { cid: 7, port: 9000 })
        .unwrap()
        .listen(1, &Pollee::new())
        .ok()
        .unwrap();
    let mut client = connect(5003, 7, 9000);
    let mut server = wait(|| listener.try_accept().ok());
    assert_eq!(client.remote_addr().cid, 7);
    assert_eq!(server.local_addr().cid, 7);
    assert_eq!(
        wait(|| server
            .check_io_events()
            .contains(crate::events::IoEvents::OUT)
            .then_some(())),
        ()
    );
    assert_eq!(client.try_send(&mut Source(b"peer"), SendFlags).unwrap(), 4);
    let mut bytes = [0u8; 8];
    let n = wait(
        || match server.try_recv(&mut Destination(&mut bytes), RecvFlags(false)) {
            Ok(n) => Some(n),
            Err(e) if e.errno == Errno::EAGAIN => None,
            Err(e) => panic!("peer recv {e:?}"),
        },
    );
    assert_eq!(n, 4);
    assert_eq!(&bytes[..4], b"peer");
    assert_eq!(
        server
            .try_send(&mut Source(&bytes[..4]), SendFlags)
            .unwrap(),
        4
    );
    let mut echoed = [0u8; 8];
    assert_eq!(
        wait(|| client
            .try_recv(&mut Destination(&mut echoed), RecvFlags(false))
            .ok()),
        4
    );
    assert_eq!(&echoed[..4], b"peer");
    drop(server);
    drop(client);
    drop(listener);
    // One attachment may own both sockets: two distinct event credits must
    // still be reserved before either Established can overwrite the other.
    let loop_listener = BoundPort::new_exclusive(VsockSocketAddr {
        cid: 42,
        port: 9100,
    })
    .unwrap()
    .listen(1, &Pollee::new())
    .ok()
    .unwrap();
    let mut loop_client = connect(5004, 42, 9100);
    let mut loop_server = wait(|| loop_listener.try_accept().ok());
    assert_eq!(
        loop_client
            .try_send(&mut Source(b"loop"), SendFlags)
            .unwrap(),
        4
    );
    let mut loop_bytes = [0u8; 8];
    assert_eq!(
        wait(|| loop_server
            .try_recv(&mut Destination(&mut loop_bytes), RecvFlags(false))
            .ok()),
        4
    );
    assert_eq!(&loop_bytes[..4], b"loop");
    drop(loop_server);
    drop(loop_client);
    drop(loop_listener);
}
