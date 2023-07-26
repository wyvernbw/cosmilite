use serde::{de::DeserializeOwned, Serialize};
use std::net::SocketAddr;

use crate::packet::Packet;

pub mod async_udp;

pub trait AsyncSocket: Sized {
    type Serializer: crate::packet::Serializer;
    async fn bind(addr: SocketAddr) -> Self;
    async fn recv<T: DeserializeOwned + Send>(&self) -> Option<crate::event::Event<T>>;
    async fn recv_filtered<T: DeserializeOwned + Send>(&self) -> crate::event::Event<T>;
    fn try_recv<T: DeserializeOwned + Send>(&self) -> Option<crate::event::Event<T>>;
    async fn send<T: Serialize + DeserializeOwned + Send + 'static>(
        &mut self,
        data: Packet<T>,
    ) -> Result<(), async_udp::SendError<<Self::Serializer as crate::packet::Serializer>::SerError>>;
    fn local_addr(&self) -> SocketAddr;
}

pub trait SyncSocket {
    type Serializer: crate::packet::Serializer;
    fn bind(addr: SocketAddr) -> Self;
    fn recv<T: DeserializeOwned + Send>(&self) -> Option<crate::event::Event<T>>;
    fn recv_filtered<T: DeserializeOwned + Send>(&self) -> crate::event::Event<T>;
    fn try_recv<T: DeserializeOwned + Send>(&self) -> Option<crate::event::Event<T>>;
    fn send<T: Serialize + DeserializeOwned + Send + 'static>(
        &mut self,
        data: Packet<T>,
    ) -> Result<(), async_udp::SendError<<Self::Serializer as crate::packet::Serializer>::SerError>>;
    fn local_addr(&self) -> SocketAddr;
}

pub trait SocketMarker {}
