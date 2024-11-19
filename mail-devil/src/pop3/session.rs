//! Structures for tracking the state of a POP3 session.

use std::{os::windows::fs::FileTypeExt, path::PathBuf};

use crate::{
    printlnif,
    state::Pop3ServerState,
    types::{Pop3Username, MAILDIR_NEW_FOLDER},
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
    pub async fn enter_transaction_state(&mut self, user_handle: UserHandle, mut maildrop_path: PathBuf) -> Option<usize> {
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

        loop {
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

        let messages_len = messages.len();
        maildrop_path.pop();
        self.state = Pop3SessionState::Transaction(TransactionState::new(maildrop_path, user_handle, messages));
        Some(messages_len)
    }
}

/// Represents the state of a POP3 session. Each client should have its own `Pop3SessionState`.
pub enum Pop3SessionState {
    Authorization(AuthorizationState),
    Transaction(TransactionState),
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
    pub maildrop_dir: PathBuf,

    /// The handle in the user tracker for the logged in user.
    pub user_handle: UserHandle,

    /// The list of messages on the user's maildrop at the time of opening it, alongisde information on each message.
    ///
    /// The messages are ordered by message number, so the message `messages[i]` has the message number `(i+1)`.
    pub messages: Vec<Message>,
}

impl TransactionState {
    pub const fn new(maildrop_dir: PathBuf, user_handle: UserHandle, messages: Vec<Message>) -> Self {
        Self {
            maildrop_dir,
            user_handle,
            messages,
        }
    }
}

/// Represents a message on a user's maildrop, alongside additional information.
pub struct Message {
    /// The location on the filesystem where this message is found.
    pub file: PathBuf,

    /// The size of the message measured in bytes.
    pub size: u64,

    /// Whether the user has requested this message to be deleted in the current session.
    pub is_deleted: bool,
}

impl Message {
    fn new(file: PathBuf, size: u64) -> Self {
        Self {
            file,
            size,
            is_deleted: false,
        }
    }
}
