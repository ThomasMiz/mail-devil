//! Structures for tracking the state of a POP3 session.

use std::path::PathBuf;

use crate::{types::Pop3Username, user_tracker::UserHandle};

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
    /// The handle in the user tracker for the logged in user.
    pub user_handle: UserHandle,

    /// The list of messages on the user's maildrop at the time of opening it, alongisde information on each message.
    pub messages: Vec<Message>,
}

impl TransactionState {
    pub const fn new(user_handle: UserHandle) -> Self {
        Self {
            user_handle,
            messages: Vec::new(),
        }
    }
}

/// Represents a message on a user's maildrop, alongside additional information.
pub struct Message {
    /// The location on the filesystem where this message is found.
    pub file: PathBuf,

    /// The size of the message measured in bytes, or [`None`] if it hasn't been calculated yet.
    pub size: Option<usize>,

    /// Whether the user has requested this message to be deleted in the current session.
    pub is_deleted: bool,
}
