use std::net::SocketAddr;

use cosmilite::{
    event::{Event, SocketEvent},
    packet::Packet,
    router::async_router::{Connection, Router, State},
    socket::{async_udp::Socket, AsyncSocket},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Galaxy {
    name: String,
    planets: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Star {
    name: String,
    age: u128,
}

#[derive(Default, Clone)]
struct Starship {
    stars_visited: u32,
    fleet: Vec<SocketAddr>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    // create a socket to run our server
    let addr = "127.0.0.1:0".parse()?;
    let socket = Socket::bind(addr).await;
    let server_addr = socket.local_addr();
    // create a router
    let starship = Starship::default();
    Router::new(socket)
        .with_state(starship)
        // Hello galaxy!
        .route(warp)
        .route(star)
        .run();
    // create a client
    tokio::spawn(async move {
        let _ = client(server_addr).await;
    })
    .await
    .unwrap();
    Ok(())
}

#[allow(unused_variables)]
async fn warp(
    Event(addr, event): Event<Galaxy>,
    State(starship): State<Starship>,
    socket: Connection,
) -> Starship {
    tracing::info!("🌌 Received galaxy, entering warp speed!");
    // send a response back
    if let SocketEvent::Received(galaxy) = event {
        let packet = Packet::new(galaxy, addr);
        // the socket is available behind a mutex
        let _ = socket.lock().await.send(packet).await;
    }
    starship
}

async fn star(Event(addr, event): Event<Star>, State(mut starship): State<Starship>) -> Starship {
    match event {
        SocketEvent::Connected => {
            tracing::info!("Friend connected!");
            starship.fleet.push(addr);
        }
        SocketEvent::Received(star) => {
            tracing::info!("✨ Received {star:?}, journaling...");
            starship.stars_visited += 1;
        }
        SocketEvent::Disconnected => (),
    }
    starship
}

async fn client(server_addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    let mut socket = Socket::bind("127.0.0.1:0".parse()?).await;
    let galaxy = Galaxy {
        name: "Andromeda".to_string(),
        planets: vec!["Earth".to_string(), "Mars".to_string()],
    };
    let packet = Packet::new(galaxy, server_addr);
    socket.send(packet).await?;
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
    loop {
        interval.tick().await;
        let star = Star {
            name: "Sun".to_string(),
            age: 4_603_000_000,
        };
        let packet = Packet::new(star, server_addr);
        socket.send(packet).await?;
    }
}
