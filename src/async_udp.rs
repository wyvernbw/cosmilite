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
    /// Sends a packet through the connection.
    ///
    /// This method sends the specified packet through the connection. The packet should be of type `Packet<SendMsg>`.
    /// If the sending operation succeeds, `Ok(())` is returned. Otherwise, if an error occurs during the sending process,
    /// a `SendError<Packet<SendMsg>>` is returned.
    ///
    /// # Arguments
    ///
    /// * `packet` - The packet to be sent through the connection.
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the packet is successfully sent.
    /// * `Err(SendError<Packet<SendMsg>>)` if an error occurs during the sending process.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use cosmilite::async_udp::Connection;
    /// use cosmilite::packet::Packet;
    /// use serde::{Deserialize, Serialize};
    ///
    /// #[derive(Serialize, Deserialize, Clone)]
    /// enum Greetings {
    ///     Hello,
    ///     Goodbye,
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     // Initialize the server connection
    ///     # let server_addr = "127.0.0.1:12345".parse().unwrap();
    ///     # let socket = cosmilite::async_udp::bind_single::<Greetings>(server_addr)
    ///     #     .await
    ///     #     .unwrap();
    ///     # let mut server_connection = socket.start_polling().await.unwrap();
    ///
    ///     // Send a packet
    ///     let packet = cosmilite::packet::Packet::new(Greetings::Hello, server_addr);
    ///     let result = server_connection.send(packet);
    ///
    ///     match result {
    ///         Ok(()) => {
    ///             // Packet sent successfully
    ///         }
    ///         Err(error) => {
    ///             // Handle send error
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// In the above example, the `send` method is used to send a `Packet` through the server connection. The packet is
    /// created using `Packet::new` with a payload of type `Greetings::Hello` and the server address. The result of the
    /// sending operation is then handled using a `match` statement to check if the packet was sent successfully or if
    /// an error occurred during the sending process. The example uses the `tokio::main` macro to run the asynchronous
    /// `main` function.
    pub fn send(&self, packet: Packet<SendMsg>) -> Result<(), SendError<Packet<SendMsg>>> {
        self.sender.send(packet)
    }

    /// Receives a packet from the established connection.
    ///
    /// This function asynchronously waits for a packet to be received from the underlying connection.
    /// It returns an `Option` that contains a `SocketEvent` representing the received packet.
    ///
    /// # Returns
    ///
    /// This function returns an `Option` that represents the received packet.
    /// - If a packet is received, `Some(SocketEvent)` is returned, where `SocketEvent` is a type representing
    ///   the received packet or an error.
    /// - If no packet is received, `None` is returned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use serde::{Deserialize, Serialize};
    /// #[derive(Serialize, Deserialize, Clone)]
    /// enum Greetings {
    ///     Hello,
    ///     Goodbye,
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     use cosmilite::packet::Packet;
    ///
    ///     use cosmilite::async_udp::Connection;
    ///     use cosmilite::events::SocketEvent;
    ///
    ///     # let server_addr = "127.0.0.1:12345".parse().unwrap();
    ///     # let socket = cosmilite::async_udp::bind_single::<Greetings>(server_addr)
    ///     #     .await
    ///     #     .unwrap();
    ///     # let mut server_connection = socket.start_polling().await.unwrap();
    ///
    ///     tokio::task::spawn(async move {
    ///         if let Some(event) = server_connection.recv().await {
    ///             match event {
    ///                 SocketEvent::Message(msg, addr) => todo!(),
    ///                 SocketEvent::Connected(addr) => todo!(),
    ///                 SocketEvent::Disconnected(addr) => todo!(),
    ///             }
    ///         }
    ///     });
    /// }
    /// ```
    pub async fn recv(&mut self) -> Option<SocketEvent<RecvMsg>> {
        self.receiver.recv().await
    }

    /// Attempts to receive a socket event from the connection without blocking.
    ///
    /// This method tries to receive a socket event from the connection. If an event is available,
    /// it returns `Ok(SocketEvent<RecvMsg>)` with the received event. If no event is currently
    /// available, it returns `Err(TryRecvError)`. This method does not block the execution and
    /// immediately returns the result.
    ///
    /// # Returns
    ///
    /// * `Ok(SocketEvent<RecvMsg>)` if a socket event is successfully received.
    /// * `Err(TryRecvError)` if no event is currently available.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use cosmilite::async_udp::Connection;
    /// use cosmilite::events::SocketEvent;
    /// use serde::{Deserialize, Serialize};
    ///
    /// #[derive(Serialize, Deserialize, Clone, Debug)]
    /// enum Greetings {
    ///     Hello,
    ///     Goodbye,
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     // Initialize the connection
    ///     let client_addr = "127.0.0.1:0".parse().unwrap();
    ///     let socket = cosmilite::async_udp::bind_single::<Greetings>(client_addr)
    ///         .await
    ///         .unwrap();
    ///     let mut connection = socket.start_polling().await.unwrap();
    ///
    ///     // Attempt to receive a socket event
    ///     match connection.try_recv() {
    ///         Ok(event) => {
    ///             // Process the received socket event
    ///             match event {
    ///                 SocketEvent::Message(msg, addr) => {
    ///                     // Handle message event
    ///                 }
    ///                 SocketEvent::Connected(addr) => {
    ///                     // Handle connected event
    ///                 }
    ///                 SocketEvent::Disconnected(addr) => {
    ///                     // Handle disconnected event
    ///                 }
    ///             }
    ///         }
    ///         Err(error) => {
    ///             // Handle try receive error
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// In the above example, the `try_recv` method is used to attempt to receive a socket event from the connection.
    /// The result is then handled using a `match` statement to check if an event is successfully received or if an error
    /// occurs when trying to receive an event. If an event is received, it is further processed based on its type. The
    /// example uses the `tokio::main` macro to run the asynchronous `main` function and includes the definition of the `Greetings`
    /// enum outside of the `main` function.
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
