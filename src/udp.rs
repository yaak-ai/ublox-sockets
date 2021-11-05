use core::cmp::min;
use fugit::{ExtU32, SecsDurationU32};

use super::{Error, Instant, Result, RingBuffer, Socket, SocketHandle, SocketMeta};
pub use embedded_nal::{Ipv4Addr, SocketAddr, SocketAddrV4};

/// A UDP socket ring buffer.
pub type SocketBuffer<const N: usize> = RingBuffer<u8, N>;

#[derive(Debug, PartialEq, Eq, Clone, Copy, defmt::Format)]
pub enum State {
    Closed,
    Established,
}

impl Default for State {
    fn default() -> Self {
        State::Closed
    }
}

/// A User Datagram Protocol socket.
///
/// A UDP socket is bound to a specific endpoint, and owns transmit and receive
/// packet buffers.
pub struct UdpSocket<const TIMER_HZ: u32, const L: usize> {
    pub(crate) meta: SocketMeta,
    pub(crate) endpoint: Option<SocketAddr>,
    check_interval: SecsDurationU32,
    read_timeout: Option<SecsDurationU32>,
    state: State,
    available_data: usize,
    rx_buffer: SocketBuffer<L>,
    last_check_time: Option<Instant<TIMER_HZ>>,
    closed_time: Option<Instant<TIMER_HZ>>,
}

impl<const TIMER_HZ: u32, const L: usize> UdpSocket<TIMER_HZ, L> {
    /// Create an UDP socket with the given buffers.
    pub fn new(socket_id: u8) -> UdpSocket<TIMER_HZ, L> {
        UdpSocket {
            meta: SocketMeta {
                handle: SocketHandle(socket_id),
            },
            check_interval: 15.secs(),
            state: State::Closed,
            read_timeout: Some(15.secs()),
            endpoint: None,
            available_data: 0,
            rx_buffer: SocketBuffer::new(),
            last_check_time: None,
            closed_time: None,
        }
    }

    /// Return the socket handle.
    pub fn handle(&self) -> SocketHandle {
        self.meta.handle
    }

    pub fn update_handle(&mut self, handle: SocketHandle) {
        self.meta.update(handle)
    }

    /// Return the bound endpoint.
    pub fn endpoint(&self) -> Option<SocketAddr> {
        self.endpoint
    }

    /// Return the connection state, in terms of the UDP connection.
    pub fn state(&self) -> State {
        self.state
    }

    pub fn set_state(&mut self, state: State) {
        defmt::debug!("{}, UDP state change: {:?} -> {:?}", self.handle(), self.state, state);
        self.state = state
    }

    pub fn should_update_available_data(&mut self, ts: Instant<TIMER_HZ>) -> bool {
        self.last_check_time
            .replace(ts)
            .and_then(|last_check_time| ts.checked_duration_since(last_check_time))
            .map(|dur| dur >= self.check_interval)
            .unwrap_or(false)
    }

    pub fn recycle(&self, ts: Instant<TIMER_HZ>) -> bool {
        if let Some(read_timeout) = self.read_timeout {
            self.closed_time
                .and_then(|closed_time| ts.checked_duration_since(closed_time))
                .map(|dur| dur >= read_timeout)
                .unwrap_or(false)
        } else {
            false
        }
    }

    pub fn closed_by_remote(&mut self, ts: Instant<TIMER_HZ>) {
        self.closed_time.replace(ts);
    }

    /// Set available data.
    pub fn set_available_data(&mut self, available_data: usize) {
        self.available_data = available_data;
    }

    /// Get the number of bytes available to ingress.
    pub fn get_available_data(&self) -> usize {
        self.available_data
    }

    pub fn rx_window(&self) -> usize {
        self.rx_buffer.window()
    }

    /// Bind the socket to the given endpoint.
    ///
    /// This function returns `Err(Error::Illegal)` if the socket was open
    /// (see [is_open](#method.is_open)), and `Err(Error::Unaddressable)`
    /// if the port in the given endpoint is zero.
    pub fn bind<T: Into<SocketAddr>>(&mut self, endpoint: T) -> Result<()> {
        if self.is_open() {
            return Err(Error::Illegal);
        }

        self.endpoint.replace(endpoint.into());
        Ok(())
    }

    /// Check whether the socket is open.
    pub fn is_open(&self) -> bool {
        self.endpoint.is_some()
    }

    /// Check whether the receive buffer is full.
    pub fn can_recv(&self) -> bool {
        !self.rx_buffer.is_full()
    }

    // /// Return the maximum number packets the socket can receive.
    // #[inline]
    // pub fn packet_recv_capacity(&self) -> usize {
    //     self.rx_buffer.packet_capacity()
    // }

    // /// Return the maximum number of bytes inside the recv buffer.
    // #[inline]
    // pub fn payload_recv_capacity(&self) -> usize {
    //     self.rx_buffer.payload_capacity()
    // }

    fn recv_impl<'b, F, R>(&'b mut self, f: F) -> Result<R>
    where
        F: FnOnce(&'b mut SocketBuffer<L>) -> (usize, R),
    {
        // We may have received some data inside the initial SYN, but until the connection
        // is fully open we must not dequeue any data, as it may be overwritten by e.g.
        // another (stale) SYN. (We do not support TCP Fast Open.)
        if !self.is_open() {
            return Err(Error::Illegal);
        }

        let (_size, result) = f(&mut self.rx_buffer);
        Ok(result)
    }

    /// Dequeue a packet received from a remote endpoint, and return the endpoint as well
    /// as a pointer to the payload.
    ///
    /// This function returns `Err(Error::Exhausted)` if the receive buffer is empty.
    pub fn recv<'b, F, R>(&'b mut self, f: F) -> Result<R>
    where
        F: FnOnce(&'b mut [u8]) -> (usize, R),
    {
        self.recv_impl(|rx_buffer| rx_buffer.dequeue_many_with(f))
    }

    /// Dequeue a packet received from a remote endpoint, copy the payload into the given slice,
    /// and return the amount of octets copied as well as the endpoint.
    ///
    /// See also [recv](#method.recv).
    pub fn recv_slice(&mut self, data: &mut [u8]) -> Result<usize> {
        self.recv_impl(|rx_buffer| {
            let size = rx_buffer.dequeue_slice(data);
            (size, size)
        })
    }

    pub fn rx_enqueue_slice(&mut self, data: &[u8]) -> usize {
        self.rx_buffer.enqueue_slice(data)
    }

    /// Peek at a packet received from a remote endpoint, and return the endpoint as well
    /// as a pointer to the payload without removing the packet from the receive buffer.
    /// This function otherwise behaves identically to [recv](#method.recv).
    ///
    /// It returns `Err(Error::Exhausted)` if the receive buffer is empty.
    pub fn peek(&mut self, size: usize) -> Result<&[u8]> {
        if !self.is_open() {
            return Err(Error::Illegal);
        }

        Ok(self.rx_buffer.get_allocated(0, size))
    }

    /// Peek at a packet received from a remote endpoint, copy the payload into the given slice,
    /// and return the amount of octets copied as well as the endpoint without removing the
    /// packet from the receive buffer.
    /// This function otherwise behaves identically to [recv_slice](#method.recv_slice).
    ///
    /// See also [peek](#method.peek).
    pub fn peek_slice(&mut self, data: &mut [u8]) -> Result<usize> {
        let buffer = self.peek(data.len())?;
        let length = min(data.len(), buffer.len());
        data[..length].copy_from_slice(&buffer[..length]);
        Ok(length)
    }

    pub fn close(&mut self) {
        self.endpoint.take();
    }
}

impl<const TIMER_HZ: u32, const L: usize> Into<Socket<TIMER_HZ, L>> for UdpSocket<TIMER_HZ, L> {
    fn into(self) -> Socket<TIMER_HZ, L> {
        Socket::Udp(self)
    }
}
