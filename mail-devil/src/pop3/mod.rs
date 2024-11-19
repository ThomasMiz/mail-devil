use std::io::{self, ErrorKind};

use inlined::TinyVec;
use parsers::{Pop3Command, MAX_COMMAND_LINE_LENGTH};
use responses::Pop3Response;
use tokio::{
    io::{AsyncWriteExt, BufReader, BufWriter},
    net::TcpStream,
    select,
};

use crate::{printlnif, state::Pop3ServerState};

mod copy;
mod handlers;
mod parsers;
mod responses;
mod session;

pub async fn handle_client(mut socket: TcpStream, server_state: Pop3ServerState) -> io::Result<()> {
    let (read_half, write_half) = socket.split();
    let mut reader = BufReader::with_capacity(server_state.buffer_size(), read_half);
    let mut writer = BufWriter::with_capacity(server_state.buffer_size(), write_half);

    let mut session = session::Pop3Session::new(server_state);

    let banner = "No swearing on my christian POP3 server";
    Pop3Response::ok(banner).write_to(&mut writer).await?;

    // An inlined buffer into which we will copy an entire line before parsing it all at once.
    let mut parse_buf: TinyVec<MAX_COMMAND_LINE_LENGTH, u8> = TinyVec::new();

    loop {
        select! {
            biased;
            result = parsers::read_line(&mut reader, &mut parse_buf) => {
                match result {
                    Err(error) if error.kind() == ErrorKind::UnexpectedEof => break,
                    Err(error) => return Err(error),
                    _ => {}
                }

                let parse_result = parsers::parse_command(&mut parse_buf);
                parse_buf.clear();

                let command = match parse_result {
                    Err(err) => {
                        Pop3Response::err(err).write_to(&mut writer).await?;
                        continue;
                    }
                    Ok(cmd) => cmd,
                };

                match command {
                    Pop3Command::User(user) => handlers::handle_user_command(&mut writer, &mut session, user).await?,
                    Pop3Command::Pass(pass) => handlers::handle_pass_command(&mut writer, &mut session, pass).await?,
                    Pop3Command::Stat => handlers::handle_stat_command(&mut writer, &mut session).await?,
                    Pop3Command::List(arg) => handlers::handle_list_command(&mut writer, &mut session, arg).await?,
                    Pop3Command::Retr(arg) => handlers::handle_retr_command(&mut writer, &mut session, arg).await?,
                    Pop3Command::Dele(arg) => handlers::handle_dele_command(&mut writer, &mut session, arg).await?,
                    Pop3Command::Noop => handlers::handle_noop_command(&mut writer, &mut session).await?,
                    Pop3Command::Rset => handlers::handle_rset_command(&mut writer, &mut session).await?,
                    Pop3Command::Quit => {
                        handlers::handle_quit_command(&mut writer, &mut session).await?;
                        writer.flush().await?;
                        break;
                    }
                }
            }
            result = writer.flush(), if !writer.buffer().is_empty() => {
                result?;
            }
        }
    }

    writer.shutdown().await?;
    printlnif!(!session.server.silent(), "Client disconnected");
    Ok(())
}
