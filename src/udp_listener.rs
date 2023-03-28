use hash32::Hash;
use heapless::{spsc::Queue, FnvIndexMap};
use no_std_net::SocketAddr;

use crate::{Error, SocketHandle};

pub struct UdpListener<const N: usize, const L: usize> {
    /// Maps Server Socket handles to ports
    handles: FnvIndexMap<SocketHandle, u16, N>,
    /// Maps Connection Sockets to remote socket address
    connections: FnvIndexMap<u16, Queue<(SocketHandle, SocketAddr), L>, N>,
}

impl<const N: usize, const L: usize> UdpListener<N, L> {
    pub fn new() -> Self {
        Self {
            handles: FnvIndexMap::new(),
            connections: FnvIndexMap::new(),
        }
    }

    /// Bind sockethandle to port, and create queue for incomming sockets
    pub fn bind(&mut self, handle: SocketHandle, port: u16) -> Result<(), Error> {
        if self.handles.contains_key(&handle) {
            return Err(Error::ListenerError);
        }

        self.handles
            .insert(handle, port)
            .map_err(|_| Error::ListenerError)?;
        self.connections
            .insert(port, Queue::new())
            .map_err(|_| Error::ListenerError)?;

        Ok(())
    }

    /// Unbind sockethandle to port, and create queue for incomming sockets
    pub fn unbind(&mut self, handle: SocketHandle) -> Result<(), Error> {
        if let Some(port) = self.handles.remove(&handle) {
            self.connections.remove(&port);
            Ok(())
        } else {
            Err(Error::ListenerError)
        }
    }

    /// Get incomming connection queue for port
    pub fn incoming(&mut self, port: u16) -> Option<&mut Queue<(SocketHandle, SocketAddr), L>> {
        self.connections.get_mut(&port)
    }

    /// Returns true if port is UDP server port
    pub fn is_port_bound(&self, port: u16) -> bool {
        self.connections.get(&port).is_some()
    }

    /// Returns true if socket is UDP server socket
    pub fn is_bound(&self, handle: SocketHandle) -> bool {
        self.handles.get(&handle).is_some()
    }

    /// See if a connection is available for server
    pub fn available(&mut self, handle: SocketHandle) -> Result<bool, Error> {
        let port = self.handles.get(&handle).ok_or(Error::ListenerError)?;
        Ok(!self
            .connections
            .get_mut(port)
            .ok_or(Error::ListenerError)?
            .is_empty())
    }

    /// Peek from queue of incomming connections for socket.
    pub fn peek_remote(
        &mut self,
        handle: SocketHandle,
    ) -> Result<&(SocketHandle, SocketAddr), Error> {
        let port = self.handles.get(&handle).ok_or(Error::ListenerError)?;
        self.connections
            .get_mut(port)
            .ok_or(Error::ListenerError)?
            .peek()
            .ok_or(Error::ListenerError)
    }

    /// Pop from queue of incomming connections for socket.
    pub fn get_remote(
        &mut self,
        handle: SocketHandle,
    ) -> Result<(SocketHandle, SocketAddr), Error> {
        let port = self.handles.get(&handle).ok_or(Error::ListenerError)?;
        self.connections
            .get_mut(port)
            .ok_or(Error::ListenerError)?
            .dequeue()
            .ok_or(Error::ListenerError)
    }

    pub fn get_port(&mut self, handle: SocketHandle) -> Result<u16, Error> {
        let port = self.handles.get(&handle).ok_or(Error::ListenerError)?;
        Ok(*port)
    }

    /// Gives an outgoing connection, if first in queue matches socketaddr
    /// Removes it from stack.
    pub fn get_outgoing(
        &mut self,
        handle: &SocketHandle,
        addr: SocketAddr,
    ) -> Option<SocketHandle> {
        let port = self.handles.get(handle)?;
        let queue = self.connections.get_mut(port)?;
        let (_, queue_addr) = queue.peek()?;
        if *queue_addr == addr {
            let (handle, _) = queue.dequeue()?;
            return Some(handle);
        }
        None
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct SocketAddrWrapper(SocketAddr);

impl Hash for SocketAddrWrapper {
    fn hash<H>(&self, state: &mut H)
    where
        H: hash32::Hasher,
    {
        match self.0 {
            SocketAddr::V4(ip) => {
                ip.ip().octets().hash(state);
                ip.port().hash(state);
            }
            SocketAddr::V6(ip) => {
                ip.ip().octets().hash(state);
                ip.port().hash(state);
            }
        }
    }
}
