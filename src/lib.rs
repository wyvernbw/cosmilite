use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
};

use packet::PacketInner;

pub mod async_udp;
pub mod common;
pub mod events;
pub mod packet;

type PacketsSent = [Option<PacketInner>; 32];

#[derive(Debug)]
pub(crate) struct SocketState {
    pub(crate) peers: HashSet<SocketAddr>,
    pub(crate) curr_ack: u8,
    pub(crate) packets_sent: [Option<PacketInner>; 32],
    pub(crate) packets_received: u32,
    pub(crate) sequence_streams: Vec<Option<u32>>,
    pub(crate) order_streams: Vec<Option<u32>>,
    pub(crate) order_cache: HashMap<u8, Vec<PacketInner>>,
}

impl Default for SocketState {
    fn default() -> Self {
        Self {
            peers: HashSet::new(),
            curr_ack: 0,
            packets_sent: PacketsSent::default(),
            packets_received: 0,
            sequence_streams: vec![None; 255],
            order_streams: vec![None; 255],
            order_cache: HashMap::new(),
        }
    }
}

impl SocketState {
    pub(crate) fn connect(&mut self, addr: SocketAddr) -> bool {
        self.peers.insert(addr)
    }
    /// Updates the acking state and returns an iterator over the packets that need to be resent.
    pub(crate) fn send_packet(
        &mut self,
        packet: &mut packet::PacketInner,
        soruce: SocketAddr,
    ) -> Box<dyn Iterator<Item = &PacketInner> + '_ + std::marker::Send> {
        packet.ack = self.packets_received;
        packet.src = Some(soruce);
        packet.idx = self.curr_ack;
        if packet.reliability == packet::Reliability::Unreliable {
            return Box::new(std::iter::empty::<&PacketInner>());
        }
        self.curr_ack += 1;
        if self.curr_ack >= 32 {
            self.curr_ack = 0;
            return Box::new(self.packets_sent.iter().flatten());
        }
        self.packets_sent[self.curr_ack as usize] = Some(packet.clone());
        Box::new(std::iter::empty::<&PacketInner>())
    }
    /// Returns an iterator over the packets that can reach the application.
    pub(crate) fn receive_packet<'a>(
        &'a mut self,
        packet: &'a packet::PacketInner,
    ) -> Box<dyn Iterator<Item = &PacketInner> + '_ + std::marker::Send> {
        self.packets_received |= packet.ack;
        match (&packet.reliability, &packet.ordering) {
            (packet::Reliability::Unreliable, None) => Box::new(std::iter::once(packet)),
            (_, Some(packet::Ordering::Sequenced)) => {
                let seq_stream = packet.seq_stream as usize;
                let stream_add = 1 << packet.idx;
                let stream = &mut self.sequence_streams[seq_stream];
                match *stream {
                    Some(ref mut stream) => {
                        let max = bit_seq_max(*stream);
                        if packet.idx > max {
                            *stream |= stream_add;
                            Box::new(std::iter::once(packet))
                        } else {
                            Box::new(std::iter::empty())
                        }
                    }
                    None => {
                        *stream = Some(stream_add);
                        Box::new(std::iter::once(packet))
                    }
                }
            }
            (packet::Reliability::Reliable, None) => Box::new(std::iter::once(packet)),
            (_, Some(packet::Ordering::Ordered)) => {
                let ord_stream = packet.seq_stream as usize;
                let stream_add = 1 << packet.idx;
                let stream = &mut self.order_streams[ord_stream];
                match *stream {
                    Some(ref mut stream) => {
                        *stream |= stream_add;
                        let max = bit_seq_max(*stream);
                        let full_seq = (0..=max)
                            .map(|idx| *stream & (1 << idx))
                            .all(|num| num != 0);
                        if full_seq {
                            self.order_streams[ord_stream] = None;
                            let packets = self.order_cache.get(&packet.seq_stream).unwrap();
                            Box::new(packets.iter())
                        } else {
                            self.push_to_order_cache(packet);
                            Box::new(std::iter::empty())
                        }
                    }
                    None => {
                        if packet.idx == 0 {
                            Box::new(std::iter::once(packet))
                        } else {
                            *stream = Some(stream_add);
                            self.push_to_order_cache(packet);
                            Box::new(std::iter::empty())
                        }
                    }
                }
            }
        }
    }
    pub fn push_to_order_cache(&mut self, packet: &PacketInner) {
        self.order_cache
            .entry(packet.seq_stream)
            .and_modify(|vec| {
                vec.push(packet.clone());
            })
            .or_insert_with(|| vec![packet.clone()]);
    }
}

fn bit_seq_max(seq: u32) -> u8 {
    let (_, idx) = (0..32)
        .map(|idx| (seq & (1 << idx), idx))
        .filter(|(num, _)| *num != 0)
        .max_by(|(_, a_idx), (_, b_idx)| a_idx.cmp(b_idx))
        .unwrap();
    idx
}
