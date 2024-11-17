use std::{
    env, io,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    process::exit,
};

use args::ArgumentsRequest;
use tokio::{
    net::{TcpListener, TcpStream},
    task::LocalSet,
};

mod args;
mod pop3;
mod util;

fn main() {
    let arguments = match args::parse_arguments(env::args()) {
        Err(err) => {
            eprintln!("{err}\n\nType 'mail-devil --help' for a help menu");
            exit(1);
        }
        Ok(arguments) => arguments,
    };

    let startup_args = match arguments {
        ArgumentsRequest::Version => {
            println!("{}", args::get_version_string());
            println!("Push Pop for now, Push Pop for later.");
            return;
        }
        ArgumentsRequest::Help => {
            println!("{}", args::get_help_string());
            return;
        }
        ArgumentsRequest::Run(startup_args) => startup_args,
    };

    println!("Startup arguments: {startup_args:?}"); // TODO: Remove

    printlnif!(startup_args.verbose, "Starting up tokio runtime");
    let start_result = tokio::runtime::Builder::new_current_thread().enable_all().build();
    let runtime = match start_result {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("Failed to start tokio runtime: {err}");
            exit(1);
        }
    };

    let result = LocalSet::new().block_on(&runtime, async_main());
    if let Err(e) = result {
        eprintln!("Server closed due to unexpected error: {e}");
        exit(1);
    }
}

async fn async_main() -> io::Result<()> {
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 110))
        .await
        .inspect_err(|err| eprintln!("Failed to bind listening socket: {err}"))?;

    let server_state = pop3::Pop3ServerState::new();

    loop {
        let (client_socket, client_address) = listener
            .accept()
            .await
            .inspect_err(|err| eprintln!("Failed to accept incoming connection: {err}"))?;

        println!("Incoming connection from {client_address}");
        tokio::task::spawn_local(handle_client_wrapper(client_socket, client_address, server_state.clone()));
    }
}

async fn handle_client_wrapper(socket: TcpStream, address: SocketAddr, server_state: pop3::Pop3ServerState) {
    if let Err(err) = pop3::handle_client(socket, server_state).await {
        eprintln!("Client from {address} ended with error: {err}");
    }
}
