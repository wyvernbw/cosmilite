use std::time::Duration;

use cosmilite::{
    packet::JsonSerializer,
    socket::{async_udp::socket, AsyncSocket},
};
use tokio::select;

#[tokio::test]
async fn multitype() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    color_eyre::install()?;

    type A = (u8, u8);
    type B = (String, u32);
    const MSG_COUNT: usize = 100;
    let addr = "127.0.0.1:0".parse()?;
    let mut client = socket(addr).serializer(JsonSerializer).bind().await;
    let server = socket(addr).serializer(JsonSerializer).bind().await;

    let server_addr = server.local_addr();
    tokio::task::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(15));
        for _ in 0..MSG_COUNT {
            let packet = match rand::random::<bool>() {
                true => client.send(cosmilite::packet::Packet::new(
                    (rand::random::<u8>(), rand::random::<u8>()),
                    server_addr,
                )),
                false => client.send(cosmilite::packet::Packet::new(
                    ("text".to_string(), rand::random::<u32>()),
                    server_addr,
                )),
            };
            interval.tick().await;
            let _ = packet.await;
        }
    });
    select! {
        _ = tokio::time::sleep(Duration::from_secs(10)) => {
            tracing::error!("Failed to receive all packets!");
            panic!("Failed to receive all packets!");
        }
        _ = tokio::task::spawn(async move {
            let mut count = 0;
            while count < MSG_COUNT {
                select! {
                    _ = server.recv::<A>() => {
                        tracing::info!("Received A");
                        count += 1;
                    }
                    _ = server.recv::<B>() => {
                        tracing::info!("Received B");
                        count += 1;
                    }
                }
                tracing::info!("Received {} packets", count);
            }
            count
        }) => {
            tracing::info!("Received all packets!");
        }
    }
    Ok(())
}
