use std::{fmt::Write, io};

use inlined::TinyString;
use tokio::io::{AsyncWrite, AsyncWriteExt, BufReader};

use crate::types::{MessageNumber, Pop3ArgString, Pop3Username};

use super::{
    copy::{self, CopyError},
    responses::Pop3Response,
    session::{GetMessageError, Pop3Session, Pop3SessionState},
};

const ONLY_ALLOWED_IN_AUTHORIZATION_STATE: &str = "Command only allowed in the AUTHORIZATION state";
const ONLY_ALLOWED_IN_TRANSACTION_STATE: &str = "Command only allowed in the TRANSACTION state";
const NO_SUCH_MESSAGE: &str = "No such message";
const MESSAGE_IS_DELETED: &str = "Message is deleted";
const ERROR_ACCESSING_FILE: &str = "Error accessing file";

pub async fn handle_user_command<W>(writer: &mut W, session: &mut Pop3Session, username: Pop3Username) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match &mut session.state {
        Pop3SessionState::Authorization(authorization_state) => {
            authorization_state.username = Some(username);
            Pop3Response::ok_empty()
        }
        _ => Pop3Response::err(ONLY_ALLOWED_IN_AUTHORIZATION_STATE),
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
        _ => Pop3Response::err(ONLY_ALLOWED_IN_AUTHORIZATION_STATE),
    };

    response.write_to(writer).await
}

pub async fn handle_quit_command<W>(writer: &mut W, session: &mut Pop3Session) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match session.quit_session().await {
        Ok(count) => Pop3Response::ok_deleted(count),
        Err(count) => Pop3Response::err_deleted(count),
    };

    response.write_to(writer).await
}

pub async fn handle_stat_command<W>(writer: &mut W, session: &mut Pop3Session) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match &mut session.state {
        Pop3SessionState::Transaction(transaction_state) => {
            let (message_count, maildrop_size) = transaction_state.get_stats().await;
            Pop3Response::ok_stat(message_count, maildrop_size)
        }
        _ => Pop3Response::Err(Some(ONLY_ALLOWED_IN_TRANSACTION_STATE)),
    };

    response.write_to(writer).await
}

pub async fn handle_list_command<W>(writer: &mut W, session: &mut Pop3Session, message_number: Option<MessageNumber>) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let error_message = match &mut session.state {
        Pop3SessionState::Transaction(transaction_state) => match message_number {
            Some(msgnum) => match transaction_state.get_message_mut(msgnum) {
                Err(GetMessageError::NotExists) => NO_SUCH_MESSAGE,
                Err(GetMessageError::Deleted) => MESSAGE_IS_DELETED,
                Ok(message) => match message.calculate_size().await {
                    Ok(s) => return Pop3Response::ok_list_one(msgnum, s).write_to(writer).await,
                    Err(_) => ERROR_ACCESSING_FILE,
                },
            },
            None => {
                transaction_state.ensure_all_sizes_loaded().await;
                Pop3Response::ok_empty().write_to(writer).await?;
                let mut buf = TinyString::<32>::new();
                let iter = transaction_state.messages().iter().enumerate().map(|(i, m)| (i + 1, m));
                for (msgnum, message) in iter.filter(|(_, m)| !m.delete_requested()) {
                    let _ = write!(buf, "{msgnum} {}\r\n", message.size().unwrap_or(0));
                    writer.write_all(buf.as_bytes()).await?;
                    buf.clear();
                }

                return writer.write_all(b".\r\n").await;
            }
        },
        _ => ONLY_ALLOWED_IN_TRANSACTION_STATE,
    };

    Pop3Response::err(error_message).write_to(writer).await
}

pub async fn handle_retr_command<W>(writer: &mut W, session: &mut Pop3Session, message_number: MessageNumber) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let error = match &session.state {
        Pop3SessionState::Transaction(transaction_state) => match transaction_state.get_message(message_number) {
            Ok(message) => match tokio::fs::File::open(message.path()).await {
                Ok(file) => {
                    Pop3Response::ok_empty().write_to(writer).await?;
                    let mut reader = BufReader::with_capacity(session.server.buffer_size(), file);
                    match copy::copy(&mut reader, writer).await {
                        Ok(()) => {}
                        Err(CopyError::WriterError(error)) => return Err(error),
                        Err(CopyError::ReaderError(error)) => {
                            eprintln!("Error while reading from file during copy: {error}");
                            return Err(error);
                        }
                    };
                    writer.write_all(b"\r\n.\r\n").await?;
                    return Ok(());
                }
                Err(error) => {
                    eprintln!("Could not open message file {} {error}", message.path().display());
                    "Error opening message file"
                }
            },
            Err(GetMessageError::NotExists) => NO_SUCH_MESSAGE,
            Err(GetMessageError::Deleted) => MESSAGE_IS_DELETED,
        },
        _ => ONLY_ALLOWED_IN_TRANSACTION_STATE,
    };

    Pop3Response::err(error).write_to(writer).await
}

pub async fn handle_dele_command<W>(writer: &mut W, session: &mut Pop3Session, message_number: MessageNumber) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match &mut session.state {
        Pop3SessionState::Transaction(transaction_state) => match transaction_state.delete_message(message_number) {
            Ok(()) => Pop3Response::ok_empty(),
            Err(GetMessageError::NotExists) => Pop3Response::err(NO_SUCH_MESSAGE),
            Err(GetMessageError::Deleted) => Pop3Response::err(MESSAGE_IS_DELETED),
        },
        _ => Pop3Response::err(ONLY_ALLOWED_IN_TRANSACTION_STATE),
    };

    response.write_to(writer).await
}

pub async fn handle_noop_command<W>(writer: &mut W, session: &mut Pop3Session) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match &session.state {
        Pop3SessionState::Transaction(_) => Pop3Response::ok_empty(),
        _ => Pop3Response::err(ONLY_ALLOWED_IN_TRANSACTION_STATE),
    };

    response.write_to(writer).await
}

pub async fn handle_rset_command<W>(writer: &mut W, session: &mut Pop3Session) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let response = match &mut session.state {
        Pop3SessionState::Transaction(transaction_state) => {
            transaction_state.reset_messages();
            Pop3Response::ok_empty()
        }
        _ => Pop3Response::err(ONLY_ALLOWED_IN_TRANSACTION_STATE),
    };

    response.write_to(writer).await
}
