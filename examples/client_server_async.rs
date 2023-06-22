use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Greetings {
    Hi,
    HiBack,
}

#[tokio::main]
async fn main() {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let server_addr = "127.0.0.1:12345".parse().unwrap();
    let socket = cosmilite::async_udp::bind_single::<Greetings>(server_addr)
        .await
        .unwrap();
    let mut server_connection = socket.start_polling().await.unwrap();

    let mut handles = vec![];

    // server task
    let handle = tokio::task::spawn(async move {
        use cosmilite::events::SocketEvent;
        let mut packet_idx = 0;
        while let Some(event) = server_connection.recv().await {
            match event {
                SocketEvent::Message(msg, addr) => {
                    tracing::info!(
                        "👍 Got message #{packet_idx}: {:?}, Sending {:?}",
                        msg,
                        Greetings::HiBack
                    );
                    packet_idx += 1;
                    let packet = cosmilite::packet::Packet::new(Greetings::HiBack, addr)
                        .reliable()
                        .ordered();
                    let _ = server_connection.send(packet);
                }
                SocketEvent::Connected(addr) => {
                    tracing::info!("✨ Client connected: {}", addr);
                }
                SocketEvent::Disconnected(_) => {}
            }
        }
    });
    handles.push(handle);

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    let client_addr = "127.0.0.1:0".parse().unwrap();
    let socket = cosmilite::async_udp::bind_single::<Greetings>(client_addr)
        .await
        .unwrap();
    let connection = socket.start_polling().await.unwrap();

    // client task
    let handle = tokio::task::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(1));
        loop {
            interval.tick().await;
            let packet = cosmilite::packet::Packet::new(Greetings::Hi, server_addr);
            let _ = connection.send(packet);
            tracing::info!("👋 Sent message: {:?}", Greetings::Hi);
        }
    });
    handles.push(handle);

    for handle in handles {
        handle.await.unwrap();
    }
}
