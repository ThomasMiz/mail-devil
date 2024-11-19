//! Structures for tracking the state of a POP3 session.

use std::{os::windows::fs::FileTypeExt, path::PathBuf};

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
        let mut messages = Vec::new();

        let mut directory_reader = tokio::fs::read_dir(&maildrop_path)
            .await
            .inspect_err(|error| eprintln!("Unexpected error while reading user {}'s maildrop: {error}", user_handle.username()))
            .ok()?;

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
            let pathd = &path.display();
            let file_type = match dir_entry.file_type().await {
                Ok(t) => t,
                Err(error) => {
                    eprintln!("Unexpected file type error for user {username}'s maildrop on file {pathd}: {error}");
                    continue;
                }
            };

            if !file_type.is_file() && !file_type.is_symlink_file() {
                printlnif!(self.server.verbose(), "Ignoring directory on {username}'s maildrop: {pathd}");
                continue;
            }

            let size = match dir_entry.metadata().await {
                Ok(m) => m.len(),
                Err(error) => {
                    eprintln!("Unexpected file type error for user {username}'s maildrop on file {pathd}: {error}");
                    continue;
                }
            };

            messages.push(Message::new(path, size));
        }

        let messages_len = messages.len() as MessageNumberCount;
        maildrop_path.pop();
        self.state = Pop3SessionState::Transaction(TransactionState::new(maildrop_path, user_handle, messages));
        Some(messages_len)
    }

    /// Quits the current session and, if in the transaction state, deletes any messages marked for deletion by moving
    /// them to the `cur` directory.
    ///
    /// Returns [`Ok`] or [`Err`] depending on whether the operation succeeded, in both cases specifying the maount of
    /// deleted messages. In all cases, the state is set to the `END` state.
    ///
    /// Will always return `Ok(0)` when not in the transaction state.
    pub async fn quit_session(&mut self) -> Result<MessageNumberCount, MessageNumberCount> {
        let mut count = 0;
        let mut is_ok = true;

        if let Pop3SessionState::Transaction(transaction_state) = &mut self.state {
            let pathbuf = &mut transaction_state.maildrop_dir;

            if transaction_state.messages.iter().any(|m| m.is_deleted) {
                pathbuf.push(MAILDIR_OLD_FOLDER);
                if let Err(error) = tokio::fs::create_dir_all(pathbuf.as_path()).await {
                    eprintln!("Could not ensure old messages folder exists: {error} on {}", pathbuf.display());
                    is_ok = false;
                } else {
                    for deleted_message in transaction_state.messages.iter().filter(|m| m.is_deleted) {
                        let deleted_message_file = match deleted_message.file.file_name() {
                            Some(f) => f,
                            None => {
                                eprintln!("Could not get file name from path {}", deleted_message.file.display());
                                is_ok = false;
                                continue;
                            }
                        };

                        pathbuf.push(deleted_message_file);
                        match tokio::fs::rename(&deleted_message.file.as_path(), pathbuf.as_path()).await {
                            Ok(()) => count += 1,
                            Err(error) => {
                                is_ok = false;
                                eprintln!(
                                    "Error moving message file to old messages folder: {error} while moving {} to {}",
                                    deleted_message.file.display(),
                                    pathbuf.display()
                                )
                            }
                        }
                        pathbuf.pop();
                    }
                }
            }
        }

        self.state = Pop3SessionState::End;

        match is_ok {
            true => Ok(count),
            false => Err(count),
        }
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
        let non_deleted_iter = self.messages.iter().filter(|m| !m.is_deleted);
        let stats_iter = non_deleted_iter.map(|m| (1 as MessageNumberCount, m.size()));
        stats_iter.reduce(|(c1, t1), (c2, t2)| (c1 + c2, t1 + t2)).unwrap_or((0, 0))
    }

    pub fn get_message(&self, message_number: MessageNumber) -> Result<&Message, GetMessageError> {
        let index = (message_number.get() - 1) as usize;

        match self.messages.get(index) {
            None => Err(GetMessageError::NotExists),
            Some(m) if m.is_deleted => Err(GetMessageError::Deleted),
            Some(m) => Ok(m),
        }
    }

    pub fn delete_message(&mut self, message_number: MessageNumber) -> Result<(), GetMessageError> {
        let index = (message_number.get() - 1) as usize;

        match self.messages.get_mut(index) {
            None => Err(GetMessageError::NotExists),
            Some(m) if m.is_deleted => Err(GetMessageError::Deleted),
            Some(m) => {
                m.is_deleted = true;
                Ok(())
            }
        }
    }

    pub fn reset_messages(&mut self) {
        for message in &mut self.messages {
            message.is_deleted = false;
        }
    }
}

/// Represents a message on a user's maildrop, alongside additional information.
pub struct Message {
    /// The location on the filesystem where this message is found.
    file: PathBuf,

    /// The size of the message measured in bytes.
    size: u64,

    /// Whether the user has requested this message to be deleted in the current session.
    is_deleted: bool,
}

impl Message {
    fn new(file: PathBuf, size: u64) -> Self {
        Self {
            file,
            size,
            is_deleted: false,
        }
    }

    pub const fn size(&self) -> u64 {
        self.size
    }

    pub const fn is_deleted(&self) -> bool {
        self.is_deleted
    }
}
