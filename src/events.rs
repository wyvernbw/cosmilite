use std::net::SocketAddr;

pub enum SocketEvent<RecvMsg> {
    Message(RecvMsg, SocketAddr),
    Connected(SocketAddr),
    Disconnected(SocketAddr),
}
