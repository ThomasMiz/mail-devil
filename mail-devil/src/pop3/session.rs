//! Structures for tracking the state of a POP3 session.

use super::parsers::Pop3ArgString;

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

pub struct AuthorizationState {
    pub username: Option<Pop3ArgString>,
}

impl AuthorizationState {
    pub const fn new() -> Self {
        Self { username: None }
    }
}

pub struct TransactionState {}

impl TransactionState {
    pub const fn new() -> Self {
        Self {}
    }
}
