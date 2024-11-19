use std::{fmt::Write, io};

use inlined::TinyString;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::types::{MessageNumber, Pop3ArgString, Pop3Username};

use super::{
    responses::Pop3Response,
    session::{Pop3Session, Pop3SessionState},
};

pub async fn handle_user_command<W>(writer: &mut W, session: &mut Pop3Session, username: Pop3Username) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match &mut session.state {
        Pop3SessionState::Authorization(authorization_state) => {
            authorization_state.username = Some(username);
            Pop3Response::ok_empty()
        }
        _ => Pop3Response::err("Command only allowed in the AUTHORIZATION state"),
    };

    response.write_to(writer).await
}

pub async fn handle_pass_command<W>(writer: &mut W, session: &mut Pop3Session, password: Pop3ArgString) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match &mut session.state {
        Pop3SessionState::Authorization(authorization_state) => match &authorization_state.username {
            None => Pop3Response::err("Must specify a user before a password"),
            Some(username) => match session.server.try_login_user(username, &password).await {
                Ok((user_handle, maildrop_path)) => match session.enter_transaction_state(user_handle, maildrop_path).await {
                    Some(_) => Pop3Response::ok_empty(),
                    None => Pop3Response::err("An unexpected error occurred while opening your maildrop"),
                },
                Err(reason) => Pop3Response::err(reason.get_reason_str()),
            },
        },
        _ => Pop3Response::err("Command only allowed in the AUTHORIZATION state"),
    };

    response.write_to(writer).await
}

pub async fn handle_quit_command<W>(writer: &mut W, session: &mut Pop3Session) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = Pop3Response::err("Not implemented :-(");

    response.write_to(writer).await
}

pub async fn handle_stat_command<W>(writer: &mut W, session: &mut Pop3Session) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match &session.state {
        Pop3SessionState::Transaction(transaction_state) => {
            let message_count = transaction_state.messages.len();
            let maildrop_size = transaction_state.messages.iter().map(|m| m.size()).sum::<u64>();
            Pop3Response::ok_stat(message_count, maildrop_size)
        }
        _ => Pop3Response::Err(Some("Command only allowed in the TRANSACTION state")),
    };

    response.write_to(writer).await
}

pub async fn handle_list_command<W>(writer: &mut W, session: &mut Pop3Session, message_number: Option<MessageNumber>) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let error_message = match &session.state {
        Pop3SessionState::Transaction(transaction_state) => match message_number {
            Some(msgnum) if msgnum.get() as usize > transaction_state.messages.len() => "No such message",
            Some(msgnum) => {
                let message = &transaction_state.messages[msgnum.get() as usize - 1];
                match message.is_deleted() {
                    true => "Message is deleted",
                    false => return Pop3Response::ok_list_one(msgnum, message.size()).write_to(writer).await,
                }
            }
            None => {
                Pop3Response::ok_empty().write_to(writer).await?;
                let mut buf = TinyString::<32>::new();
                let iter = transaction_state.messages.iter().enumerate().map(|(i, m)| (i + 1, m));
                for (msgnum, message) in iter.filter(|(_, m)| !m.is_deleted()) {
                    let _ = write!(buf, "{msgnum} {}\r\n", message.size());
                    writer.write_all(buf.as_bytes()).await?;
                    buf.clear();
                }

                return Ok(());
            }
        },
        _ => "Command only allowed in the TRANSACTION state",
    };

    Pop3Response::err(error_message).write_to(writer).await
}

pub async fn handle_retr_command<W>(writer: &mut W, session: &mut Pop3Session, message_number: MessageNumber) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match &session.state {
        Pop3SessionState::Transaction(transaction_state) => Pop3Response::err("Not implemented :-("),
        _ => Pop3Response::err("Command only allowed in the TRANSACTION state"),
    };

    response.write_to(writer).await
}

pub async fn handle_dele_command<W>(writer: &mut W, session: &mut Pop3Session, message_number: MessageNumber) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match &mut session.state {
        Pop3SessionState::Transaction(transaction_state) => Pop3Response::err("Not implemented :-("),
        _ => Pop3Response::err("Command only allowed in the TRANSACTION state"),
    };

    response.write_to(writer).await
}

pub async fn handle_noop_command<W>(writer: &mut W, session: &mut Pop3Session) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match &session.state {
        Pop3SessionState::Transaction(_) => Pop3Response::Ok(None),
        _ => Pop3Response::err("Command only allowed in the TRANSACTION state"),
    };

    response.write_to(writer).await
}

pub async fn handle_rset_command<W>(writer: &mut W, session: &mut Pop3Session) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match &mut session.state {
        Pop3SessionState::Transaction(transaction_state) => Pop3Response::err("Not implemented :-("),
        _ => Pop3Response::err("Command only allowed in the TRANSACTION state"),
    };

    response.write_to(writer).await
}
