use std::collections::HashMap;
use std::io::{self, ErrorKind};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use crate::args::StartupArguments;
use crate::pop3::Pop3ArgString;
use crate::state::Pop3ServerState;
use crate::util::sockets::{AcceptFromAny, PrintSockaddrOrUnknown};
use crate::{pop3, printlnif};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

pub async fn run_server(startup_args: StartupArguments) -> io::Result<()> {
    let verbose = startup_args.verbose;
    let silent = startup_args.silent;

    for (username, password) in &startup_args.users {
        if let Err(error) = create_user_maildir(verbose, silent, &startup_args.maildirs_file, username, password).await {
            eprintln!("Could not create or update user {username} as requested via parameter: {error}");
        }
    }

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

    let server_state = Pop3ServerState::new(startup_args.verbose, startup_args.silent, startup_args.maildirs_file, None);

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

async fn create_user_maildir(verbose: bool, silent: bool, maildirs_file: &Path, username: &str, password: &str) -> io::Result<()> {
    /// Create the user's maildrop directory if it doesn't exist.
    let mut path = maildirs_file.to_path_buf();
    path.push(username);
    tokio::fs::create_dir_all(&path).await?;

    // Create a "password" file in the user's maildrop and write the password to that file.
    path.push("password");
    let mut file = tokio::fs::File::create(path).await?;
    file.write_all(password.as_bytes()).await?;
    file.flush().await?;

    printlnif!(!silent, "Successfully created or updated user {username}");
    Ok(())
}

async fn handle_client_wrapper(socket: TcpStream, address: SocketAddr, server_state: Pop3ServerState) {
    if let Err(err) = pop3::handle_client(socket, server_state).await {
        eprintln!("Client from {address} ended with error: {err}");
    }
}
