use super::SocketHandle;

/// Network socket metadata.
///
/// This includes things that only external (to the socket, that is) code
/// is interested in, but which are more conveniently stored inside the socket itself.
#[derive(Debug, Default)]
pub struct Meta {
    /// Handle of this socket within its enclosing `SocketSet`.
    /// Mainly useful for debug output.
    pub(crate) handle: SocketHandle,
}

impl Meta {
    pub fn update(&mut self, handle: SocketHandle) {
        self.handle = handle;
    }
}
