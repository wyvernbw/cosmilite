use std::{marker::PhantomData, net::SocketAddr};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{
    self,
    error::{SendError, TryRecvError},
    unbounded_channel,
};

use crate::{
    common::{BincodeSerder, Config, Serder},
    events::SocketEvent,
    packet::{Packet, PacketInner},
    SocketState,
};

type UnboundedChannel<T> = (mpsc::UnboundedSender<T>, mpsc::UnboundedReceiver<T>);

pub struct Socket<SendMsg, RecvMsg, S = BincodeSerder>
where
    SendMsg: serde::Serialize,
    RecvMsg: serde::de::DeserializeOwned,
    S: Serder,
{
    socket: tokio::net::UdpSocket,
    send_channel: UnboundedChannel<Packet<SendMsg>>,
    recv_channel: UnboundedChannel<SocketEvent<RecvMsg>>,
    send_msg: PhantomData<SendMsg>,
    recv_msg: PhantomData<RecvMsg>,
    config: Config<S>,
    state: SocketState,
}

pub struct Connection<SendMsg, RecvMsg>
where
    SendMsg: serde::Serialize,
    RecvMsg: serde::de::DeserializeOwned,
{
    sender: mpsc::UnboundedSender<Packet<SendMsg>>,
    receiver: mpsc::UnboundedReceiver<SocketEvent<RecvMsg>>,
}

impl<SendMsg, RecvMsg> Socket<SendMsg, RecvMsg>
where
    SendMsg: serde::Serialize + std::marker::Send + 'static + Clone,
    RecvMsg: for<'de> serde::Deserialize<'de> + std::marker::Send + 'static,
{
    /// Create a new socket bound to the given address.
    pub async fn bind(addr: SocketAddr) -> Result<Socket<SendMsg, RecvMsg>, std::io::Error> {
        let socket = tokio::net::UdpSocket::bind(addr).await?;
        tracing::info!("🦑 Bound to {}", socket.local_addr()?);
        let (send_tx, send_rx) = mpsc::unbounded_channel::<Packet<SendMsg>>();
        let (recv_tx, recv_rx) = unbounded_channel::<SocketEvent<RecvMsg>>();
        Ok(Socket {
            socket,
            send_channel: (send_tx, send_rx),
            recv_channel: (recv_tx, recv_rx),
            send_msg: PhantomData,
            recv_msg: PhantomData,
            state: SocketState::default(),
            config: Config::<BincodeSerder>::default(),
        })
    }
    pub fn with_config<S>(self, config: Config<S>) -> Socket<SendMsg, RecvMsg, S>
    where
        S: Serder,
    {
        Socket {
            config,
            socket: self.socket,
            send_channel: self.send_channel,
            recv_channel: self.recv_channel,
            send_msg: self.send_msg,
            recv_msg: self.recv_msg,
            state: self.state,
        }
    }
    /// Consumes the socket, starting the poll and returning a connection.
    /// The connection can be used to send and receive messages.
    pub async fn start_polling(self) -> Result<Connection<SendMsg, RecvMsg>, std::io::Error> {
        let (send_tx, mut send_rx) = self.send_channel;
        let (recv_tx, recv_rx) = self.recv_channel;
        tokio::task::spawn(async move {
            let socket = self.socket;
            let config = self.config;
            let mut state = self.state;
            let mut buf = [0u8; 1024];
            loop {
                tokio::select! {
                    Some(send_packet) = send_rx.recv() => {
                        // Send a message
                        let dest = send_packet.dest;
                        let mut send_packet = send_packet.into_inner(&config);
                        let lost_packets = state.send_packet(&mut send_packet, socket.local_addr().unwrap());
                        let serialized_packet = bincode::serialize(&send_packet).unwrap();
                        // resend lost packets
                        for packet in lost_packets {
                            let serialized_packet = bincode::serialize(&packet).unwrap();
                            if socket.send_to(&serialized_packet, dest).await.is_err() {
                                break;
                            }
                        }
                        if socket.send_to(&serialized_packet, dest).await.is_err() {
                            break;
                        }
                    }
                    Ok((len, addr)) = socket.recv_from(&mut buf) => {
                        // Receive a message
                        let packet = &buf[..len];
                        let Ok(packet) = bincode::deserialize::<PacketInner>(packet) else {
                            continue;
                        };
                        if state.connect(addr) && recv_tx.send(SocketEvent::Connected(addr)).is_err() {
                            break;
                        }
                        let received_packets = state.receive_packet(&packet);
                        // holy shit
                        if received_packets.map(|packet| {
                            (config.deserialize::<RecvMsg>(&packet.data), packet.src)
                        })
                        .filter_map(|(data, src)| match (data, src) {
                            (Ok(data), Some(src)) => Some((data, src)),
                            _ => None,
                        }).try_for_each(|(data, src)| {
                            recv_tx.send(SocketEvent::Message(data, src))
                        }).is_err() {
                            break;
                        };
                    }
                }
            }
        });
        Ok(Connection {
            sender: send_tx,
            receiver: recv_rx,
        })
    }
}

impl<SendMsg, RecvMsg> Connection<SendMsg, RecvMsg>
where
    SendMsg: serde::Serialize + std::marker::Send + 'static,
    RecvMsg: serde::de::DeserializeOwned + std::marker::Send + 'static,
{
    /// Sends a message to the remote endpoint.
    ///
    /// This method attempts to send the provided `msg` to the remote endpoint.
    ///
    /// # Errors
    ///
    /// Returns a `SendError` if the message failed to be sent.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use tokio::sync::mpsc;
    /// # #[derive(serde::Serialize, serde::Deserialize)] struct MyMessage {}
    /// # #[tokio::main] async fn main() {
    /// # let (sender, mut receiver) = mpsc::channel(100);
    /// # let connection = Connection::new(sender, receiver);
    /// let message = MyMessage { /* Initialize your message */ };
    /// if let Err(err) = connection.send(message) {
    ///     eprintln!("Failed to send message: {:?}", err);
    /// }
    /// # }
    /// ```
    pub fn send(&self, packet: Packet<SendMsg>) -> Result<(), SendError<Packet<SendMsg>>> {
        self.sender.send(packet)
    }

    /// Receives a message from the remote endpoint.
    ///
    /// This method asynchronously waits for a message to be received from the remote endpoint.
    /// It returns `Some(received_message)` when a message is received, or `None` if the connection has been closed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use tokio::sync::mpsc;
    /// # #[derive(serde::Serialize, serde::Deserialize)] struct MyMessage {}
    /// # #[tokio::main] async fn main() {
    /// # let (sender, mut receiver) = mpsc::channel(100);
    /// # let connection = Connection::new(sender, receiver);
    /// if let Some(received_message) = connection.recv().await {
    ///     // Process the received message
    /// }
    /// # }
    /// ```
    pub async fn recv(&mut self) -> Option<SocketEvent<RecvMsg>> {
        self.receiver.recv().await
    }

    /// Tries to receive a message from the remote endpoint without blocking.
    ///
    /// This method attempts to receive a message from the remote endpoint without blocking. It returns a `Result` indicating whether a message was successfully received or an error occurred.
    ///
    /// # Errors
    ///
    /// Returns a `TryRecvError` if no message is currently available for reception. The error indicates that the receiver channel is empty.
    ///
    /// # Examples
    ///
    /// ```rust
    /// match connection.try_recv() {
    ///     Ok(SocketEvent::Message(msg)) => {
    ///         // Process the received message
    ///     }
    ///     Err(TryRecvError) => {
    ///         // Handle the case when no message is available
    ///     }
    /// }
    /// ```
    pub fn try_recv(&mut self) -> Result<SocketEvent<RecvMsg>, TryRecvError> {
        self.receiver.try_recv()
    }
}

pub async fn bind_single<Msg>(addr: SocketAddr) -> Result<Socket<Msg, Msg>, std::io::Error>
where
    Msg: Serialize + for<'de> Deserialize<'de> + std::marker::Send + 'static + Clone,
{
    Socket::bind(addr).await
}
