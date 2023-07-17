use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};
use std::net::SocketAddr;

use crate::packet::Packet;

pub mod async_udp;

#[async_trait]
pub trait AsyncSocket<S: crate::packet::Serializer> {
    async fn bind(addr: SocketAddr) -> Self;
    async fn recv<T: DeserializeOwned + Send>(&self) -> Option<crate::event::Event<T>>;
    async fn recv_filtered<T: DeserializeOwned + Send>(&self) -> crate::event::Event<T>;
    async fn send<T: Serialize + DeserializeOwned + Send + 'static>(
        &mut self,
        data: Packet<T>,
    ) -> Result<(), async_udp::SendError<S::SerError>>;
    fn local_addr(&self) -> SocketAddr;
}
