use nix::sys::socket::listen as listen_vsock;
use nix::sys::socket::{accept, bind, recv, send, connect, shutdown, socket};
use nix::sys::socket::{AddressFamily, Shutdown, SockFlag, SockType};
use nix::unistd::close;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use nix::sys::socket::SockAddr;
use std::io::{Error, ErrorKind, Read, Result as ResultIO, Write};
use nix::errno::Errno::EINTR;
use nix::sys::socket::MsgFlags;

pub struct Vsock{
    pub fd:RawFd
}

#[derive(Debug, Clone)]
pub struct VsockListener {
    socket: RawFd,
}

#[derive(Debug, Clone)]
pub struct VsockStream {
    socket: RawFd,
}

impl VsockListener {
    pub fn bind(addr: &SockAddr) -> Result<VsockListener,String> {
        const VMADDR_CID_ANY: u32 = 0xFFFFFFFF;
        const BACKLOG: usize = 128;

        let socket_fd = socket(
            AddressFamily::Vsock,
            SockType::Stream,
            SockFlag::empty(),
            None,
        )
            .map_err(|err| format!("Create socket failed: {:?}", err))?;
        
        bind(socket_fd, &addr).map_err(|err| format!("Bind failed: {:?}", err))?;

        listen_vsock(socket_fd, BACKLOG).map_err(|err| format!("Listen failed: {:?}", err))?;
        
        Ok(Self{
            socket: socket_fd
        })
    }

    pub fn accept(&self) -> Result<VsockStream,String> {
        let fd = accept(self.socket).map_err(|err| format!("Accept failed: {:?}", err))?;
        Ok(VsockStream{
            socket: fd
        })
    }
}

impl VsockStream{
    pub fn connect(addr: &SockAddr) -> Result<Self,String> {
        let mut err_msg = String::new();

        for i in 0..5 {
            let socket_fd = socket(
                AddressFamily::Vsock,
                SockType::Stream,
                SockFlag::empty(),
                None,
            )
                .map_err(|err| format!("Create socket failed: {:?}", err))?;
            match connect(socket_fd, &addr) {
                Ok(_) => return Ok(Self{ socket: socket_fd }),
                Err(e) => err_msg = format!("Failed to connect: {}", e),
            }

            // Exponentially backoff before retrying to connect to the socket
            std::thread::sleep(std::time::Duration::from_secs(1 << i));
        }

        Err(err_msg)
    }
}

impl Read for VsockStream {
    fn read(&mut self, buf: &mut [u8]) -> ResultIO<usize> {
        <&Self>::read(&mut &*self, buf)
    }
}

impl Write for VsockStream {
    fn write(&mut self, buf: &[u8]) -> ResultIO<usize> {
        <&Self>::write(&mut &*self, buf)
    }

    fn flush(&mut self) -> ResultIO<()> {
        Ok(())
    }
}

impl Read for &VsockStream {
    fn read(&mut self, buf: &mut [u8]) -> ResultIO<usize> {
        let ret = unsafe { recv(self.socket, buf, MsgFlags::empty()) };
        match ret {
            Ok(size) => Ok(size),
            Err(nix::Error::Sys(EINTR)) => Ok(0),
            Err(err) => return Err(Error::last_os_error()),
        }
    }
}

impl Write for &VsockStream {
    fn write(&mut self, buf: &[u8]) -> ResultIO<usize> {
        let ret = unsafe {
            send(
                self.socket,
                buf,
                MsgFlags::empty(),
            )
        };
        match ret {
            Ok(size) => Ok(size),
            Err(nix::Error::Sys(EINTR)) => Ok(0),
            Err(err) => return Err(Error::last_os_error()),
        }
    }

    fn flush(&mut self) -> ResultIO<()> {
        Ok(())
    }
}

impl AsRawFd for VsockStream {
    fn as_raw_fd(&self) -> RawFd {
        self.socket
    }
}