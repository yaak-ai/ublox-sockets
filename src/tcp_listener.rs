use embedded_nal::SocketAddr;
use heapless::{spsc::Queue, FnvIndexMap};

use crate::SocketHandle;

pub struct TcpListener<const N: usize, const L: usize> {
    handles: FnvIndexMap<SocketHandle, u16, N>,
    connections: FnvIndexMap<u16, Queue<(SocketHandle, SocketAddr), L>, N>,
}

impl<const N: usize, const L: usize> TcpListener<N, L> {
    pub fn new() -> Self {
        Self {
            handles: FnvIndexMap::new(),
            connections: FnvIndexMap::new(),
        }
    }

    pub fn bind(&mut self, handle: SocketHandle, port: u16) -> Result<(), ()> {
        if self.handles.contains_key(&handle) {
            return Err(());
        }

        self.handles.insert(handle, port).map_err(drop)?;
        self.connections.insert(port, Queue::new()).map_err(drop)?;

        Ok(())
    }

    pub fn incoming(&mut self, port: u16) -> Option<&mut Queue<(SocketHandle, SocketAddr), L>> {
        self.connections.get_mut(&port)
    }

    pub fn available(&mut self, handle: SocketHandle) -> Result<bool, ()> {
        let port = self.handles.get(&handle).ok_or(())?;
        Ok(!self.connections.get_mut(port).ok_or(())?.is_empty())
    }

    pub fn accept(&mut self, handle: SocketHandle) -> Result<(SocketHandle, SocketAddr), ()> {
        let port = self.handles.get(&handle).ok_or(())?;
        self.connections
            .get_mut(port)
            .ok_or(())?
            .dequeue()
            .ok_or(())
    }
}
