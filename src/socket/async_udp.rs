use std::{fmt::Display, net::SocketAddr};

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};

use crate::packet::PacketInner;

use super::{AsyncSocket, SocketMarker};

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

impl Socket {
    pub async fn bind(addr: SocketAddr) -> Self {
        use tokio::net::UdpSocket;
        let socket = UdpSocket::bind(addr).await.unwrap();
        tracing::info!("🦑 Socket bound to {}!", socket.local_addr().unwrap());
        Socket {
            socket,
            serializer: bincode::DefaultOptions::new(),
        }
    }
}

impl<S: crate::packet::Serializer + Send + Sync> AsyncSocket for Socket<S> {
    type Serializer = S;
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
    fn try_recv<T: DeserializeOwned + Send>(&self) -> Option<crate::event::Event<T>> {
        use crate::event::Event;
        use crate::event::SocketEvent;

        let mut buf = [0; 1024];
        let Ok((len, addr)) = self.socket.try_peek_from(&mut buf) else {
			return None;
		};
        let data = &buf[..len];
        let inner_packet = bincode::deserialize_from::<_, PacketInner>(data).unwrap();
        let data = S::deserialize::<T>(&inner_packet.data);
        if let Some(data) = data {
            self.socket
                .try_recv_from(&mut buf)
                .map(|_| Event(addr, SocketEvent::Received(data)))
                .ok()
        } else {
            None
        }
    }
    /// Asynchronously receives data from a socket and deserializes it into an event of type `Event<T>`.
    ///
    /// Returns an `Option` containing an `Event<T>` if data is successfully received and deserialized.
    /// Returns `None` if a message of a different type is received first. Said message needs to be handled separately. If you want to discard it, use `recv_filtered<T>()` instead.
    ///
    /// # Example
    ///
    /// The `recv<T>()` function is typically used in an asynchronous context to receive and handle events of different types.
    ///
    /// ```rust
    /// # use crate::event::{Event, SocketEvent};
    /// #
    /// # async fn example(server: &Server) {
    /// loop {
    ///     if let Some(Event(_addr, SocketEvent::Received(dog))) = server.recv::<Dog>().await {
    ///         tracing::info!("🐕 Received dog: {:?}", dog);
    ///     }
    ///     if let Some(Event(_addr, SocketEvent::Received(cat))) = server.recv::<Cat>().await {
    ///         tracing::info!("🐈 Received cat: {:?}", cat);
    ///     }
    /// }
    /// # }
    /// ```
    ///
    /// In this example, the function is called within a loop to continuously receive and handle events.
    /// The type parameter `<T>` represents the type of event expected to be received.
    ///
    /// # Behavior
    ///
    /// 1. The function first attempts to receive data from the socket.
    /// 2. If data is available, it deserializes the received data using the `bincode` crate and extracts the inner packet.
    /// 3. It then attempts to deserialize the inner packet's data into the specified type `T`.
    /// 4. If the deserialization succeeds, it receives the complete data from the socket and constructs an `Event<T>` with the received data and the source address.
    /// 5. If the deserialization fails or no data is available, it returns `None`.
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

impl SocketMarker for Socket {}
