use std::{io, ops::Deref};

use tokio::io::AsyncWrite;

use super::{
    parsers::{MessageNumber, Pop3ArgString},
    responses::Pop3Response,
    session::{Pop3SessionState, TransactionState},
    Pop3ServerState,
};

pub async fn handle_user_command<W>(
    writer: &mut W,
    session_state: &mut Pop3SessionState,
    server_state: &Pop3ServerState,
    username: Pop3ArgString,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match session_state {
        Pop3SessionState::Authorization(authorization_state) => {
            authorization_state.username = Some(username);
            Pop3Response::Ok(None)
        }
        _ => Pop3Response::Err(Some("Command only allowed in the AUTHORIZATION state")),
    };

    response.write_to(writer).await
}

pub async fn handle_pass_command<W>(
    writer: &mut W,
    session_state: &mut Pop3SessionState,
    server_state: &Pop3ServerState,
    password: Pop3ArgString,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match session_state {
        Pop3SessionState::Authorization(authorization_state) => match &authorization_state.username {
            None => Pop3Response::Err(Some("Must specify a user before a password")),
            Some(username) => {
                if username.deref().eq("pedro") && password.deref().eq("other123") {
                    // TODO: Implement users
                    *session_state = Pop3SessionState::Transaction(TransactionState::new(username.clone()));
                    Pop3Response::Ok(None)
                } else {
                    Pop3Response::Err(Some("Invalid user or password"))
                }
            }
        },
        _ => Pop3Response::Err(Some("Command only allowed in the AUTHORIZATION state")),
    };

    response.write_to(writer).await
}

pub async fn handle_quit_command<W>(writer: &mut W, session_state: &mut Pop3SessionState, server_state: &Pop3ServerState) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = Pop3Response::Err(Some("Not implemented :-("));

    response.write_to(writer).await
}

pub async fn handle_stat_command<W>(writer: &mut W, session_state: &mut Pop3SessionState, server_state: &Pop3ServerState) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match session_state {
        Pop3SessionState::Transaction(transaction_state) => Pop3Response::Err(Some("Not implemented :-(")),
        _ => Pop3Response::Err(Some("Command only allowed in the TRANSACTION state")),
    };

    response.write_to(writer).await
}

pub async fn handle_list_command<W>(
    writer: &mut W,
    session_state: &mut Pop3SessionState,
    server_state: &Pop3ServerState,
    message_number: Option<MessageNumber>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match session_state {
        Pop3SessionState::Transaction(transaction_state) => Pop3Response::Err(Some("Not implemented :-(")),
        _ => Pop3Response::Err(Some("Command only allowed in the TRANSACTION state")),
    };

    response.write_to(writer).await
}

pub async fn handle_retr_command<W>(
    writer: &mut W,
    session_state: &mut Pop3SessionState,
    server_state: &Pop3ServerState,
    message_number: MessageNumber,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match session_state {
        Pop3SessionState::Transaction(transaction_state) => Pop3Response::Err(Some("Not implemented :-(")),
        _ => Pop3Response::Err(Some("Command only allowed in the TRANSACTION state")),
    };

    response.write_to(writer).await
}

pub async fn handle_dele_command<W>(
    writer: &mut W,
    session_state: &mut Pop3SessionState,
    server_state: &Pop3ServerState,
    message_number: MessageNumber,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match session_state {
        Pop3SessionState::Transaction(transaction_state) => Pop3Response::Err(Some("Not implemented :-(")),
        _ => Pop3Response::Err(Some("Command only allowed in the TRANSACTION state")),
    };

    response.write_to(writer).await
}

pub async fn handle_noop_command<W>(writer: &mut W, session_state: &mut Pop3SessionState, server_state: &Pop3ServerState) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match session_state {
        Pop3SessionState::Transaction(_) => Pop3Response::Ok(None),
        _ => Pop3Response::Err(Some("Command only allowed in the TRANSACTION state")),
    };

    response.write_to(writer).await
}

pub async fn handle_rset_command<W>(writer: &mut W, session_state: &mut Pop3SessionState, server_state: &Pop3ServerState) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match session_state {
        Pop3SessionState::Transaction(transaction_state) => Pop3Response::Err(Some("Not implemented :-(")),
        _ => Pop3Response::Err(Some("Command only allowed in the TRANSACTION state")),
    };

    response.write_to(writer).await
}
