use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use crate::common::{Config, Serder};

#[derive(Debug)]
pub struct Packet<T> {
    pub data: T,
    pub dest: SocketAddr,
    pub(crate) reliability: Reliability,
    pub(crate) ordering: Option<Ordering>,
    pub(crate) seq_stream: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacketInner {
    pub data: Vec<u8>,
    pub reliability: Reliability,
    pub ordering: Option<Ordering>,
    pub seq_stream: u8,
    pub ack: u32,
    pub idx: u8,
    pub src: Option<SocketAddr>,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum Reliability {
    Reliable,
    Unreliable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Ordering {
    Sequenced,
    Ordered,
}

impl<T: Serialize> Packet<T> {
    pub fn new(data: T, dest: SocketAddr) -> Self {
        Self {
            data,
            dest,
            reliability: Reliability::Unreliable,
            ordering: None,
            seq_stream: 0,
        }
    }
    pub fn reliable(self) -> Packet<T> {
        Packet {
            reliability: Reliability::Reliable,
            ..self
        }
    }
    pub fn sequenced(self, stream: u8) -> Packet<T> {
        Packet {
            ordering: Some(Ordering::Sequenced),
            seq_stream: stream,
            ..self
        }
    }
    pub fn ordered(self) -> Packet<T> {
        Packet {
            ordering: Some(Ordering::Ordered),
            reliability: Reliability::Reliable,
            ..self
        }
    }
    pub fn unordered(self) -> Packet<T> {
        Packet {
            ordering: None,
            ..self
        }
    }
    pub(crate) fn into_inner<S: Serder>(self, config: &Config<S>) -> PacketInner {
        PacketInner {
            data: config.serialize(&self.data).unwrap(),
            reliability: self.reliability,
            ordering: self.ordering,
            seq_stream: self.seq_stream,
            ack: 0,
            src: None,
            idx: 0,
        }
    }
}
