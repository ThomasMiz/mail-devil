use std::{env, process::exit};

use args::ArgumentsRequest;
use tokio::task::LocalSet;

mod args;
mod pop3;
mod server;
mod state;
mod types;
mod user_tracker;
mod util;

fn main() {
    let arguments = match args::parse_arguments(env::args()) {
        Err(err) => {
            eprintln!("{err}\n\nType 'mail-devil --help' for a help menu");
            exit(1);
        }
        Ok(arguments) => arguments,
    };

    let mut startup_args = match arguments {
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

    if startup_args.silent && startup_args.verbose {
        eprintln!("This absolute jackass requested both silent and verbose. I'm disabling verbose.");
        startup_args.verbose = false;
    }

    printlnif!(startup_args.verbose, "Starting up tokio runtime");
    let start_result = tokio::runtime::Builder::new_current_thread().enable_all().build();
    let runtime = match start_result {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("Failed to start tokio runtime: {err}");
            exit(1);
        }
    };

    // Run the server's entrypoint on a `LocalSet`, then wait for any remaining tasks to wrap up.
    let localset = LocalSet::new();
    let result = localset.block_on(&runtime, server::run_server(startup_args));
    if let Err(err) = &result {
        eprintln!("{err}");
    }

    runtime.block_on(localset);

    if result.is_err() {
        exit(1);
    }
}
