use std::{error::Error, time::Duration};

use cosmilite::{
    event::Event,
    packet::{JsonSerializer, Packet},
    socket::{async_udp::socket, AsyncSocket},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();
    color_eyre::install()?;

    let addr = "127.0.0.1:0".parse()?;
    let mut client = socket(addr).serializer(JsonSerializer).bind().await;
    let server = socket(addr).serializer(JsonSerializer).bind().await;
    let server_addr = server.local_addr();
    tokio::task::spawn(async move {
        for idx in (0..5).map(|idx| Packet::new(idx, server_addr)) {
            let _ = client.send(idx).await;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
    let mut noise = socket(addr).serializer(JsonSerializer).bind().await;
    let mut interval = tokio::time::interval(Duration::from_millis(250));
    // Noise task
    // This task will send a packet every 250ms, that we will filter out
    tokio::task::spawn(async move {
        loop {
            interval.tick().await;
            let _ = noise
                .send(Packet::new(
                    "haha check this noise out".to_string(),
                    server_addr,
                ))
                .await;
        }
    });
    loop {
        use cosmilite::event::SocketEvent;

        // `recv_filtered` will return an event for every packet received, but will filter out
        // packets that don't match the type we're looking for. Filtered packets are discarded
        // and cannot be received again.
        //
        // If you need to receive packets of multiple types, use `recv` instead.
        match server.recv_filtered::<i32>().await {
            Event(addr, SocketEvent::Connected) => tracing::info!("Connected to {}", addr),
            Event(addr, SocketEvent::Disconnected) => {
                tracing::info!("Disconnected from {}", addr)
            }
            Event(addr, SocketEvent::Received(data)) => {
                tracing::info!("Received {} from {}", data, addr)
            }
        }
    }
}
