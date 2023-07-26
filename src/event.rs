use std::net::SocketAddr;

#[derive(Debug, Clone, Copy)]
pub enum SocketEvent<T> {
    Connected,
    Disconnected,
    Received(T),
}

impl<T> SocketEvent<T> {
    pub fn is_connected(&self) -> bool {
        matches!(self, SocketEvent::Connected)
    }
    pub fn is_disconnected(&self) -> bool {
        matches!(self, SocketEvent::Disconnected)
    }
    pub fn is_received(&self) -> bool {
        matches!(self, SocketEvent::Received(_))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Event<T>(pub SocketAddr, pub SocketEvent<T>);
