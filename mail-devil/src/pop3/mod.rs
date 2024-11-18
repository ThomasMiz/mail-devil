use std::io::{self, ErrorKind};

use parsers::{Pop3Command, Pop3CommandError};
use responses::Pop3Response;
use tokio::{
    io::{AsyncWriteExt, BufReader, BufWriter},
    net::TcpStream,
};

use crate::state::Pop3ServerState;

mod handlers;
mod parsers;
mod responses;
mod session;

pub use parsers::Pop3ArgString;

pub async fn handle_client(mut socket: TcpStream, server_state: Pop3ServerState) -> io::Result<()> {
    let (read_half, write_half) = socket.split();
    let mut reader = BufReader::new(read_half);
    let mut writer = BufWriter::new(write_half);

    let mut session_state = session::Pop3SessionState::new();

    let banner = "No swearing on my christian POP3 server";
    Pop3Response::Ok(Some(banner)).write_to(&mut writer).await?;

    loop {
        writer.flush().await?;
        let resulty = parsers::parse_command(&mut reader).await;

        let command = match resulty {
            Err(Pop3CommandError::IO(e)) if e.kind() == ErrorKind::UnexpectedEof => break,
            Err(Pop3CommandError::IO(e)) => return Err(e),
            Err(err) => {
                Pop3Response::Err(Some(err)).write_to(&mut writer).await?;
                continue;
            }
            Ok(cmd) => cmd,
        };

        match command {
            Pop3Command::User(user) => handlers::handle_user_command(&mut writer, &mut session_state, &server_state, user).await?,
            Pop3Command::Pass(pass) => handlers::handle_pass_command(&mut writer, &mut session_state, &server_state, pass).await?,
            Pop3Command::Quit => handlers::handle_quit_command(&mut writer, &mut session_state, &server_state).await?,
            Pop3Command::Stat => handlers::handle_stat_command(&mut writer, &mut session_state, &server_state).await?,
            Pop3Command::List(arg) => handlers::handle_list_command(&mut writer, &mut session_state, &server_state, arg).await?,
            Pop3Command::Retr(arg) => handlers::handle_retr_command(&mut writer, &mut session_state, &server_state, arg).await?,
            Pop3Command::Dele(arg) => handlers::handle_dele_command(&mut writer, &mut session_state, &server_state, arg).await?,
            Pop3Command::Noop => handlers::handle_noop_command(&mut writer, &mut session_state, &server_state).await?,
            Pop3Command::Rset => handlers::handle_rset_command(&mut writer, &mut session_state, &server_state).await?,
        }
    }

    println!("Bro is gon 💀");
    socket.shutdown().await?;
    Ok(())
}
