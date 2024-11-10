use std::{
    io,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    process::exit,
};

use tokio::{
    net::{TcpListener, TcpStream},
    task::LocalSet,
};

fn main() {
    let start_result = tokio::runtime::Builder::new_current_thread().enable_all().build();
    let runtime = match start_result {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("Failed to start tokio runtime: {err}");
            exit(1);
        }
    };

    let result = LocalSet::new().block_on(&runtime, async_main());
    if result.is_err() {
        exit(1);
    }
}

async fn async_main() -> io::Result<()> {
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 1080))
        .await
        .inspect_err(|err| eprintln!("Failed to bind listening socket: {err}"))?;

    loop {
        let (client_socket, client_address) = listener
            .accept()
            .await
            .inspect_err(|err| eprintln!("Failed to accept incoming connection: {err}"))?;

        println!("Incoming connection from {client_address}");
        tokio::task::spawn_local(handle_client_wrapper(client_socket, client_address));
    }
}

async fn handle_client_wrapper(socket: TcpStream, address: SocketAddr) {
    if let Err(err) = handle_client(socket).await {
        eprintln!("Client from {address} ended with error: {err}");
    }
}

async fn handle_client(mut socket: TcpStream) -> io::Result<()> {
    let (mut read_half, mut write_half) = socket.split();
    tokio::io::copy(&mut read_half, &mut write_half).await?;
    Ok(())
}
