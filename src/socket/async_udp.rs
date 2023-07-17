use std::{fmt::Display, net::SocketAddr};

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};

use crate::packet::PacketInner;

use super::AsyncSocket;

pub struct Socket<S: crate::packet::Serializer = bincode::DefaultOptions> {
    socket: tokio::net::UdpSocket,
    serializer: S,
}

pub struct SocketBuilder<S: crate::packet::Serializer = bincode::DefaultOptions> {
    addr: SocketAddr,
    serializer: S,
}

pub fn socket(addr: SocketAddr) -> SocketBuilder {
    SocketBuilder::new(addr)
}

impl SocketBuilder {
    pub fn new(addr: SocketAddr) -> Self {
        SocketBuilder {
            addr,
            serializer: bincode::DefaultOptions::new(),
        }
    }
}

impl<S: crate::packet::Serializer> SocketBuilder<S> {
    pub fn serializer<S2: crate::packet::Serializer>(self, serializer: S2) -> SocketBuilder<S2> {
        SocketBuilder {
            addr: self.addr,
            serializer,
        }
    }
    pub async fn bind(self) -> Socket<S> {
        use tokio::net::UdpSocket;
        let socket = UdpSocket::bind(self.addr).await.unwrap();
        tracing::info!("🦑 Socket bound to {}!", socket.local_addr().unwrap());
        Socket {
            socket,
            serializer: self.serializer,
        }
    }
}

#[derive(Debug)]
pub enum SendError<SerError> {
    Serialize(SerError),
    IoError(std::io::Error),
}

impl<S: std::fmt::Debug> Display for SendError<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SendError::Serialize(e) => write!(f, "Failed to serialize packet: {:?}", e),
            SendError::IoError(e) => write!(f, "Failed to send packet: {:?}", e),
        }
    }
}

impl<SerError: std::fmt::Debug> std::error::Error for SendError<SerError> {}

#[async_trait]
impl<S: crate::packet::Serializer + Send + Sync> AsyncSocket<S> for Socket<S> {
    fn local_addr(&self) -> SocketAddr {
        self.socket.local_addr().unwrap()
    }
    async fn bind(addr: SocketAddr) -> Self {
        use tokio::net::UdpSocket;
        let socket = UdpSocket::bind(addr).await.unwrap();
        tracing::info!("🦑 Socket bound to {}!", socket.local_addr().unwrap());
        Socket {
            socket,
            serializer: S::new(),
        }
    }
    async fn recv_filtered<T: DeserializeOwned + Send>(&self) -> crate::event::Event<T> {
        use crate::event::Event;
        use crate::event::SocketEvent;

        let mut buf = [0; 1024];
        let (addr, data) = loop {
            let (len, addr) = self.socket.recv_from(&mut buf).await.unwrap();
            let data = &buf[..len];
            let inner_packet = bincode::deserialize_from::<_, PacketInner>(data).unwrap();
            let data = S::deserialize::<T>(&inner_packet.data);
            if let Some(data) = data {
                break (addr, data);
            }
        };
        Event(addr, SocketEvent::Received(data))
    }
    async fn recv<T: DeserializeOwned + Send>(&self) -> Option<crate::event::Event<T>> {
        use crate::event::Event;
        use crate::event::SocketEvent;

        let mut buf = [0; 1024];
        let (len, addr) = self.socket.peek_from(&mut buf).await.unwrap();
        let data = &buf[..len];
        let inner_packet = bincode::deserialize_from::<_, PacketInner>(data).unwrap();
        let data = S::deserialize::<T>(&inner_packet.data);
        if let Some(data) = data {
            self.socket.recv_from(&mut buf).await.unwrap();
            Some(Event(addr, SocketEvent::Received(data)))
        } else {
            None
        }
    }
    async fn send<T: Serialize + DeserializeOwned + Send>(
        &mut self,
        packet: crate::packet::Packet<T>,
    ) -> Result<(), SendError<S::SerError>> {
        let dest = packet.dest;
        let packet = packet
            .into_inner(&self.serializer)
            .map_err(SendError::Serialize)?;
        let packet = bincode::serialize(&packet).unwrap();
        self.socket
            .send_to(&packet, dest)
            .await
            .map_err(SendError::IoError)?;
        Ok(())
    }
}
