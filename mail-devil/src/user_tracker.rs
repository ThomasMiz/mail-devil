//! An user tracker is a structure that tracks which users are currentoy logged into the server.
//!
//! The tracker internally stores a set of all the users, but instead of relying on server-related tasks to check,
//! store and then release the values from the user set, the user tracker provides a handle type that, when droped,
//! will automatically release the user in question. This definitively avoids the issue of "I forgot to remove the user
//! from the set!", as well as handle any race conditions or access-between-await-bounds issues.

use std::{
    cell::RefCell,
    collections::HashSet,
    ops::{Deref, DerefMut},
    rc::Rc,
};

use crate::pop3::Pop3ArgString;

/// A user tracker. Read the [`crate::user_tracker`] module's documentation for more information.
///
/// This is a reference type which may be cloned to create multiple references to the same state.
#[derive(Clone)]
pub struct UserTracker {
    user_set: Rc<RefCell<HashSet<Pop3ArgString>>>,
}

impl UserTracker {
    /// Creates a new [`UserTracker`] with no users.
    pub fn new() -> Self {
        Self {
            user_set: Rc::new(RefCell::new(HashSet::new())),
        }
    }

    /// Attempt to register a user into the [`UserTracker`]. Return [`Some`] with the registered user's handle on
    /// success, or [`None`] if the user is already registered.
    pub fn try_register(&self, username: Pop3ArgString) -> Option<UserHandle> {
        let mut guard = self.user_set.borrow_mut();
        match guard.insert(username.clone()) {
            true => Some(UserHandle::new(username, self.clone())),
            false => None,
        }
    }
}

/// Represents an existing user in a [`UserTracker`]. The user is automatically removed from the tracker once this
/// handle is dropped.
pub struct UserHandle {
    username: Pop3ArgString,
    tracker: UserTracker,
}

impl UserHandle {
    fn new(username: Pop3ArgString, tracker: UserTracker) -> Self {
        Self { username, tracker }
    }

    pub const fn username(&self) -> &Pop3ArgString {
        &self.username
    }
}

impl Drop for UserHandle {
    fn drop(&mut self) {
        let mut guard = self.tracker.user_set.borrow_mut();
        guard.remove(&self.username);
    }
}
