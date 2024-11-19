//! Structures for tracking the state of a POP3 session.

use std::path::{Path, PathBuf};

use tokio::{
    fs::File,
    io::{AsyncBufReadExt, BufReader},
};

use crate::{
    printlnif,
    state::Pop3ServerState,
    types::{MessageNumber, MessageNumberCount, Pop3Username, MAILDIR_NEW_FOLDER, MAILDIR_OLD_FOLDER},
    user_tracker::UserHandle,
};

/// Represents a POP3 session, with a state and a reference to the server' state.
pub struct Pop3Session {
    pub server: Pop3ServerState,
    pub state: Pop3SessionState,
}

impl Pop3Session {
    pub const fn new(server: Pop3ServerState) -> Pop3Session {
        Self {
            server,
            state: Pop3SessionState::new(),
        }
    }

    /// Reads the given user's maildir, assigns numbers to each message, and if all operations succeed transitions this
    /// session to the `TRANSACTION` state and returns [`Some`] with the amount of new messages.
    ///
    /// Returns [`None`] if a problem occurs while reading the user's maildrop.
    pub async fn enter_transaction_state(&mut self, user_handle: UserHandle, mut maildrop_path: PathBuf) -> Option<MessageNumberCount> {
        maildrop_path.push(MAILDIR_NEW_FOLDER);

        printlnif!(
            !self.server.silent(),
            "Opening user's {} maildrop at {}",
            user_handle.username(),
            maildrop_path.display()
        );

        let username = user_handle.username();

        let mut directory_reader = tokio::fs::read_dir(&maildrop_path)
            .await
            .inspect_err(|error| eprintln!("Unexpected error while reading user {}'s maildrop: {error}", user_handle.username()))
            .ok()?;

        let mut message_task_handles = Vec::new();

        // Just in case, we only load the first `MessageNumberCount::MAX` messages.
        while message_task_handles.len() < MessageNumberCount::MAX as usize {
            let dir_entry = match directory_reader.next_entry().await {
                Ok(Some(d)) => d,
                Ok(None) => break,
                Err(error) => {
                    eprintln!("Unexpected directory error for user {username}'s maildrop: {error}");
                    continue;
                }
            };

            let path = dir_entry.path();
            let handle = tokio::task::spawn_local(read_message_file_and_size(path));
            message_task_handles.push(handle);
        }

        let mut messages = Vec::with_capacity(message_task_handles.len());
        for handle in message_task_handles {
            if let Ok(Ok(message)) = handle.await {
                messages.push(message);
            }
        }

        let messages_len = messages.len() as MessageNumberCount;
        maildrop_path.pop();
        self.state = Pop3SessionState::Transaction(TransactionState::new(maildrop_path, user_handle, messages));
        Some(messages_len)
    }
}

/// Opens a message's file in read-only mode and calculates the size of the message, converting LF line endings to CRLF
/// as required by the POP3 protocol.
///
/// On success, return [`Ok`] with the given path, the opened file, and the calculated size in a new [`Message`]. On
/// error, returns an empty [`Err`] and everything is dropped.
async fn read_message_file_and_size(path: PathBuf) -> Result<Message, ()> {
    let mut file = tokio::fs::File::open(&path)
        .await
        .inspect_err(|error| eprintln!("Could not open file for reading {}: {error}", path.display()))
        .map_err(|_| ())?;

    let mut reader = BufReader::new(&mut file);
    let mut file_size = 0;
    let mut was_last_char_cr = false;
    loop {
        let buf = match reader.fill_buf().await {
            Ok([]) => break,
            Ok(b) => b,
            Err(error) => {
                eprintln!("Error while reading from file {}: {error}", path.display());
                return Err(());
            }
        };

        file_size += buf.len();
        for b in buf {
            if *b == b'\n' && !was_last_char_cr {
                file_size += 1;
            }
            was_last_char_cr = *b == b'\r';
        }

        let buf_len = buf.len();
        reader.consume(buf_len);
    }

    Ok(Message::new(path, file, file_size as u64))
}

impl Pop3Session {
    /// Quits the current session and, if in the transaction state, deletes any messages marked for deletion by moving
    /// them to the `cur` directory.
    ///
    /// Returns [`Ok`] or [`Err`] depending on whether the operation succeeded, in both cases specifying the maount of
    /// deleted messages. In all cases, the state is set to the `END` state.
    ///
    /// Will always return `Ok(0)` when not in the transaction state.
    pub async fn quit_session(&mut self) -> Result<MessageNumberCount, MessageNumberCount> {
        let old_state = std::mem::replace(&mut self.state, Pop3SessionState::End);

        match old_state {
            Pop3SessionState::Transaction(transaction_state) => handle_close_transaction(transaction_state).await,
            _ => Ok(0),
        }
    }
}

async fn handle_close_transaction(transaction_state: TransactionState) -> Result<MessageNumberCount, MessageNumberCount> {
    if !transaction_state.messages.iter().any(|m| m.delete_requested) {
        return Ok(0);
    }

    let mut pathbuf = transaction_state.maildrop_dir;
    pathbuf.push(MAILDIR_OLD_FOLDER);
    if let Err(error) = tokio::fs::create_dir_all(pathbuf.as_path()).await {
        eprintln!("Could not ensure old messages folder exists: {error} on {}", pathbuf.display());
        return Err(0);
    }

    let mut count = 0;
    let mut is_ok = true;
    for deleted_message in transaction_state.messages.iter().filter(|m| m.delete_requested) {
        let deleted_message_file = match deleted_message.path.file_name() {
            Some(f) => f,
            None => {
                eprintln!("Could not get file name from path {}", deleted_message.path.display());
                is_ok = false;
                continue;
            }
        };

        pathbuf.push(deleted_message_file);
        match tokio::fs::rename(&deleted_message.path.as_path(), pathbuf.as_path()).await {
            Ok(()) => count += 1,
            Err(error) => {
                is_ok = false;
                eprintln!(
                    "Error moving message file to old messages folder: {error} while moving {} to {}",
                    deleted_message.path.display(),
                    pathbuf.display()
                )
            }
        }
        pathbuf.pop();
    }

    match is_ok {
        true => Ok(count),
        false => Err(count),
    }
}

/// Represents the state of a POP3 session. Each client should have its own `Pop3SessionState`.
pub enum Pop3SessionState {
    Authorization(AuthorizationState),
    Transaction(TransactionState),
    End,
}

impl Pop3SessionState {
    /// Creates a [`Pop3SessionState`] for a new connection in the `AUTHORIZATION` state.
    pub const fn new() -> Self {
        Self::Authorization(AuthorizationState::new())
    }
}

/// Represents the state of a POP3 session in the `AUTHORIZATION` state.
pub struct AuthorizationState {
    /// The username specified with the `USER` command, or [`None`] of no username was specified yet.
    pub username: Option<Pop3Username>,
}

impl AuthorizationState {
    pub const fn new() -> Self {
        Self { username: None }
    }
}

/// Represents the state of a POP3 session in the `TRANSACTION` state.
pub struct TransactionState {
    /// The currently open maildrop's directory on the filesystem.
    maildrop_dir: PathBuf,

    /// The handle in the user tracker for the logged in user. This is not accessed but must be present here so the
    /// user's exclusive lock is automatically released when this handle is dropped.
    _user_handle: UserHandle,

    /// The list of messages on the user's maildrop at the time of opening it, alongisde information on each message.
    ///
    /// The messages are ordered by message number, so the message `messages[i]` has the message number `(i+1)`.
    messages: Vec<Message>,
}

pub enum GetMessageError {
    NotExists,
    Deleted,
}

impl TransactionState {
    pub const fn new(maildrop_dir: PathBuf, user_handle: UserHandle, messages: Vec<Message>) -> Self {
        Self {
            maildrop_dir,
            _user_handle: user_handle,
            messages,
        }
    }

    pub const fn messages(&self) -> &Vec<Message> {
        &self.messages
    }

    pub fn get_stats(&self) -> (MessageNumberCount, u64) {
        let non_deleted_iter = self.messages.iter().filter(|m| !m.delete_requested);
        let stats_iter = non_deleted_iter.map(|m| (1 as MessageNumberCount, m.size()));
        stats_iter.reduce(|(c1, t1), (c2, t2)| (c1 + c2, t1 + t2)).unwrap_or((0, 0))
    }

    pub fn get_message(&self, message_number: MessageNumber) -> Result<&Message, GetMessageError> {
        let index = (message_number.get() - 1) as usize;

        match self.messages.get(index) {
            None => Err(GetMessageError::NotExists),
            Some(m) if m.delete_requested => Err(GetMessageError::Deleted),
            Some(m) => Ok(m),
        }
    }

    pub fn get_message_mut(&mut self, message_number: MessageNumber) -> Result<&mut Message, GetMessageError> {
        let index = (message_number.get() - 1) as usize;

        match self.messages.get_mut(index) {
            None => Err(GetMessageError::NotExists),
            Some(m) if m.delete_requested => Err(GetMessageError::Deleted),
            Some(m) => Ok(m),
        }
    }

    pub fn delete_message(&mut self, message_number: MessageNumber) -> Result<(), GetMessageError> {
        let index = (message_number.get() - 1) as usize;

        match self.messages.get_mut(index) {
            None => Err(GetMessageError::NotExists),
            Some(m) if m.delete_requested => Err(GetMessageError::Deleted),
            Some(m) => {
                m.delete_requested = true;
                Ok(())
            }
        }
    }

    pub fn reset_messages(&mut self) {
        for message in &mut self.messages {
            message.delete_requested = false;
        }
    }
}

/// Represents a message on a user's maildrop, alongside additional information.
pub struct Message {
    /// The location on the filesystem where this message is found.
    path: PathBuf,

    /// A file opened in read mode for this message. This locks the message, blocking access to other programs.
    file: File,

    /// The size of the message measured in bytes.
    size: u64,

    /// Whether the user has requested this message to be deleted in the current session.
    delete_requested: bool,
}

impl Message {
    fn new(path: PathBuf, file: File, size: u64) -> Self {
        Self {
            path,
            file,
            size,
            delete_requested: false,
        }
    }

    pub const fn size(&self) -> u64 {
        self.size
    }

    pub const fn delete_requested(&self) -> bool {
        self.delete_requested
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn file(&mut self) -> &mut File {
        &mut self.file
    }
}
