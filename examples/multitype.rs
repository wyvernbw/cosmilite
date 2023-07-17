use std::time::Duration;

use cosmilite::{
    event::{Event, SocketEvent},
    packet::{JsonSerializer, Packet},
    socket::{async_udp::socket, AsyncSocket},
};
use serde::{Deserialize, Serialize};

/// Title: Multitype Example

#[derive(Debug, Serialize, Deserialize)]
pub struct Dog {
    pub name: String,
    pub good_boy: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Cat {
    pub name: String,
    pub meow_count: u32,
    pub is_hungry: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    color_eyre::install()?;

    let addr = "127.0.0.1:0".parse()?;
    let mut client = socket(addr).serializer(JsonSerializer).bind().await;
    let server = socket(addr).serializer(JsonSerializer).bind().await;

    let server_addr = server.local_addr();
    tokio::task::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        loop {
            interval.tick().await;
            let send_op = match rand::random::<bool>() {
                // send a dog
                true => client.send(Packet::new(
                    Dog {
                        name: "woofers".to_string(),
                        good_boy: true,
                    },
                    server_addr,
                )),
                // send a cat
                false => client.send(Packet::new(
                    Cat {
                        name: "meowy".to_string(),
                        meow_count: 5,
                        is_hungry: false,
                    },
                    server_addr,
                )),
            };
            let _ = send_op.await;
        }
    });

    loop {
        if let Some(Event(_addr, SocketEvent::Received(dog))) = server.recv::<Dog>().await {
            tracing::info!("🐕 Received dog: {:?}", dog)
        }
        if let Some(Event(_addr, SocketEvent::Received(cat))) = server.recv::<Cat>().await {
            tracing::info!("🐈 Received cat: {:?}", cat)
        }
    }
}
