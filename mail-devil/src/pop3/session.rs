//! Structures for tracking the state of a POP3 session.

use std::{
    io,
    os::windows::fs::FileTypeExt,
    path::{Path, PathBuf},
};

use tokio::io::{AsyncBufReadExt, BufReader};

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

        let mut messages = Vec::new();

        // Just in case, we only load the first `MessageNumberCount::MAX` messages.
        while messages.len() < MessageNumberCount::MAX as usize {
            let dir_entry = match directory_reader.next_entry().await {
                Ok(Some(d)) => d,
                Ok(None) => break,
                Err(error) => {
                    eprintln!("Unexpected directory error for user {username}'s maildrop: {error}");
                    continue;
                }
            };

            let path = dir_entry.path();
            let file_type = match dir_entry.file_type().await {
                Ok(t) => t,
                Err(error) => {
                    eprintln!("Unexpected error getting file type of {}: {error}", path.display());
                    continue;
                }
            };

            if file_type.is_file() || file_type.is_symlink_file() {
                messages.push(Message::new(path));
            }
        }

        let messages_len = messages.len() as MessageNumberCount;
        maildrop_path.pop();
        self.state = Pop3SessionState::Transaction(TransactionState::new(maildrop_path, user_handle, messages));
        Some(messages_len)
    }
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

    pub async fn get_stats(&mut self) -> (MessageNumberCount, u64) {
        self.ensure_all_sizes_loaded().await;

        let non_deleted_iter = self.messages.iter().filter(|m| !m.delete_requested);
        let stats_iter = non_deleted_iter.map(|m| (1 as MessageNumberCount, m.size.unwrap_or(0)));
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

    pub async fn ensure_all_sizes_loaded(&mut self) {
        if self.messages.iter().all(|m| m.size.is_some()) {
            return;
        }

        // Asynchronously calculate the size of all messages (who don't have their size cached) at the same time.
        let mut handles = Vec::with_capacity(self.messages.len());
        for message in &mut self.messages.iter().filter(|m| !m.delete_requested) {
            let maybe_size = message.size;
            let path = message.path.clone();
            handles.push(tokio::task::spawn_local(async move {
                match maybe_size {
                    Some(size) => Ok(size),
                    None => calculate_message_size(&path).await,
                }
            }));
        }

        for (handle, message) in handles.into_iter().zip(self.messages.iter_mut().filter(|m| !m.delete_requested)) {
            if let Ok(Ok(size)) = handle.await {
                message.size = Some(size);
            }
        }
    }
}

/// Represents a message on a user's maildrop, alongside additional information.
pub struct Message {
    /// The location on the filesystem where this message is found.
    path: PathBuf,

    /// The size of the message measured in bytes, or [`None`] if it hasn't been calculated yet.
    size: Option<u64>,

    /// Whether the user has requested this message to be deleted in the current session.
    delete_requested: bool,
}

impl Message {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            size: None,
            delete_requested: false,
        }
    }

    pub const fn size(&self) -> Option<u64> {
        self.size
    }

    /// Gets this message's size, calculating it if not already cached by traversing this message's file, converting LF
    /// line endings to CRLF.
    ///
    /// The file is not modified; we simply count LF line endings as if they were CRLF.
    pub async fn calculate_size(&mut self) -> io::Result<u64> {
        if let Some(file_size) = self.size {
            return Ok(file_size);
        }

        let file_size = calculate_message_size(&self.path).await?;
        self.size = Some(file_size);
        Ok(file_size)
    }

    pub const fn delete_requested(&self) -> bool {
        self.delete_requested
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

async fn calculate_message_size(path: &Path) -> io::Result<u64> {
    let file = tokio::fs::File::open(path)
        .await
        .inspect_err(|error| eprintln!("Could not open file for reading {}: {error}", path.display()))?;

    let mut reader = BufReader::new(file);
    let mut file_size = 0;
    let mut was_last_char_cr = false;

    loop {
        let buf = match reader.fill_buf().await {
            Ok([]) => break,
            Ok(b) => b,
            Err(error) => {
                eprintln!("Error while reading from file {}: {error}", path.display());
                return Err(error);
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

    Ok(file_size as u64)
}
