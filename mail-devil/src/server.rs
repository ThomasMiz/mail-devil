use std::io::{self, ErrorKind};
use std::net::SocketAddr;

use crate::args::StartupArguments;
use crate::state::Pop3ServerState;
use crate::util::sockets::{AcceptFromAny, PrintSockaddrOrUnknown};
use crate::{pop3, printlnif};
use tokio::net::{TcpListener, TcpStream};

pub async fn run_server(startup_args: StartupArguments) -> io::Result<()> {
    let mut listeners = Vec::with_capacity(startup_args.pop3_bind_sockets.len());

    for sockaddr in startup_args.pop3_bind_sockets {
        match TcpListener::bind(sockaddr).await {
            Ok(l) => listeners.push(l),
            Err(err) => eprintln!("Failed to bind listening socket at {sockaddr}: {err}"),
        }
    }

    if listeners.is_empty() {
        return Err(io::Error::new(
            ErrorKind::Other,
            "Failed to bind any listening sockets, aborting server",
        ));
    }

    let server_state = Pop3ServerState::new();

    loop {
        match listeners.accept_from_any().await {
            Ok((socket, address)) => {
                printlnif!(startup_args.verbose, "Incoming connection from {address}");
                tokio::task::spawn_local(handle_client_wrapper(socket, address, server_state.clone()));
            }
            Err((listener_index, error)) => {
                let listener = listeners.swap_remove(listener_index);
                let listener_addr = PrintSockaddrOrUnknown(listener.local_addr().ok());
                eprintln!("Error while accepting incoming connection from listener {listener_addr}: {error}");
                drop(listener);
            }
        }
    }
}

async fn handle_client_wrapper(socket: TcpStream, address: SocketAddr, server_state: Pop3ServerState) {
    if let Err(err) = pop3::handle_client(socket, server_state).await {
        eprintln!("Client from {address} ended with error: {err}");
    }
}
