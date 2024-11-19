use std::io;

use tokio::io::AsyncWrite;

use crate::types::{MessageNumber, Pop3ArgString, Pop3Username};

use super::{
    responses::Pop3Response,
    session::{Pop3Session, Pop3SessionState, TransactionState},
};

pub async fn handle_user_command<W>(writer: &mut W, session: &mut Pop3Session, username: Pop3Username) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match &mut session.state {
        Pop3SessionState::Authorization(authorization_state) => {
            authorization_state.username = Some(username);
            Pop3Response::Ok(None)
        }
        _ => Pop3Response::Err(Some("Command only allowed in the AUTHORIZATION state")),
    };

    response.write_to(writer).await
}

pub async fn handle_pass_command<W>(writer: &mut W, session: &mut Pop3Session, password: Pop3ArgString) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match &mut session.state {
        Pop3SessionState::Authorization(authorization_state) => match &authorization_state.username {
            None => Pop3Response::Err(Some("Must specify a user before a password")),
            Some(username) => match session.server.try_login_user(username, &password).await {
                Ok((user_handle, maildrop_path)) => match session.enter_transaction_state(user_handle, maildrop_path).await {
                    Some(_) => Pop3Response::Ok(None),
                    None => Pop3Response::Err(Some("An unexpected error occurred while opening your maildrop")),
                },
                Err(reason) => Pop3Response::Err(Some(reason.get_reason_str())),
            },
        },
        _ => Pop3Response::Err(Some("Command only allowed in the AUTHORIZATION state")),
    };

    response.write_to(writer).await
}

pub async fn handle_quit_command<W>(writer: &mut W, session: &mut Pop3Session) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = Pop3Response::Err(Some("Not implemented :-("));

    response.write_to(writer).await
}

pub async fn handle_stat_command<W>(writer: &mut W, session: &mut Pop3Session) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match &mut session.state {
        Pop3SessionState::Transaction(transaction_state) => Pop3Response::Err(Some("Not implemented :-(")),
        _ => Pop3Response::Err(Some("Command only allowed in the TRANSACTION state")),
    };

    response.write_to(writer).await
}

pub async fn handle_list_command<W>(writer: &mut W, session: &mut Pop3Session, message_number: Option<MessageNumber>) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match &mut session.state {
        Pop3SessionState::Transaction(transaction_state) => Pop3Response::Err(Some("Not implemented :-(")),
        _ => Pop3Response::Err(Some("Command only allowed in the TRANSACTION state")),
    };

    response.write_to(writer).await
}

pub async fn handle_retr_command<W>(writer: &mut W, session: &mut Pop3Session, message_number: MessageNumber) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match &mut session.state {
        Pop3SessionState::Transaction(transaction_state) => Pop3Response::Err(Some("Not implemented :-(")),
        _ => Pop3Response::Err(Some("Command only allowed in the TRANSACTION state")),
    };

    response.write_to(writer).await
}

pub async fn handle_dele_command<W>(writer: &mut W, session: &mut Pop3Session, message_number: MessageNumber) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match &mut session.state {
        Pop3SessionState::Transaction(transaction_state) => Pop3Response::Err(Some("Not implemented :-(")),
        _ => Pop3Response::Err(Some("Command only allowed in the TRANSACTION state")),
    };

    response.write_to(writer).await
}

pub async fn handle_noop_command<W>(writer: &mut W, session: &mut Pop3Session) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match &mut session.state {
        Pop3SessionState::Transaction(_) => Pop3Response::Ok(None),
        _ => Pop3Response::Err(Some("Command only allowed in the TRANSACTION state")),
    };

    response.write_to(writer).await
}

pub async fn handle_rset_command<W>(writer: &mut W, session: &mut Pop3Session) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match &mut session.state {
        Pop3SessionState::Transaction(transaction_state) => Pop3Response::Err(Some("Not implemented :-(")),
        _ => Pop3Response::Err(Some("Command only allowed in the TRANSACTION state")),
    };

    response.write_to(writer).await
}
