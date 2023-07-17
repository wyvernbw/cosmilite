use std::net::SocketAddr;

pub enum SocketEvent<T> {
    Connected,
    Disconnected,
    Received(T),
}

pub struct Event<T>(pub SocketAddr, pub SocketEvent<T>);
