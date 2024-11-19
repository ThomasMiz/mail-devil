//! This module contains types for managing the POP3 server's state, as well as logic for interacting with it.

use std::{path::PathBuf, rc::Rc};

use tokio::io::AsyncReadExt;

use crate::{
    printlnif,
    types::{Pop3ArgString, Pop3Username, MAX_COMMAND_ARG_LENGTH, PASSWORD_FILE_NAME},
    user_tracker::{UserHandle, UserTracker},
};

/// Stores the POP3 server's state.
///
/// This is a reference type which may be cloned to create multiple references to the same state.
#[derive(Clone)]
pub struct Pop3ServerState {
    rc: Rc<InnerState>,
}

impl Pop3ServerState {
    pub fn new(verbose: bool, silent: bool, buffer_size: u32, maildirs_dir: PathBuf, transformer_file: Option<PathBuf>) -> Self {
        Self {
            rc: Rc::new(InnerState::new(verbose, silent, buffer_size, maildirs_dir, transformer_file)),
        }
    }

    pub fn verbose(&self) -> bool {
        self.rc.verbose
    }

    pub fn silent(&self) -> bool {
        self.rc.silent
    }

    pub fn buffer_size(&self) -> usize {
        self.rc.buffer_size as usize
    }

    /// Attempts to log in as the given user with the given password.
    ///
    /// On success, returns the user's handle on the user tracker and the path to the user's maildrop.
    pub async fn try_login_user(&self, username: &Pop3Username, password: &Pop3ArgString) -> Result<(UserHandle, PathBuf), LoginUserError> {
        // Read the password file for the user into a `buf` buffer.
        let mut path = self.rc.maildirs_dir.to_path_buf();
        path.push(username.as_str());
        path.push(PASSWORD_FILE_NAME);

        let mut file = match tokio::fs::File::open(&path).await {
            Ok(f) => f,
            Err(error) => {
                printlnif!(
                    !self.silent(),
                    "Failed to login user {username}, could not open password file: {error}"
                );
                return Err(LoginUserError::WrongUserOrPass);
            }
        };

        let mut buf = [0u8; MAX_COMMAND_ARG_LENGTH];
        let mut buf_len = 0;

        while buf_len < buf.len() {
            let bytes_read = match file.read(&mut buf[buf_len..]).await {
                Ok(b) => b,
                Err(error) => {
                    printlnif!(
                        !self.silent(),
                        "Failed to login user {username}, error while reading password file: {error}"
                    );
                    return Err(LoginUserError::WrongUserOrPass);
                }
            };

            if bytes_read == 0 {
                break;
            }

            buf_len += bytes_read;
        }
        drop(file);

        if !password.as_bytes().eq(&buf[..buf_len]) {
            printlnif!(!self.silent(), "Wrong login for user {username}");
            return Err(LoginUserError::WrongUserOrPass);
        }

        let user_tracker = &self.rc.current_users;
        let user_handle = user_tracker.try_register(username.clone()).ok_or(LoginUserError::AlreadyLoggedIn)?;

        printlnif!(!self.silent(), "User {username} logged in successfully");
        path.pop();
        Ok((user_handle, path))
    }
}

/// Stores the immutable variables of a POP3 server's state.
struct InnerState {
    verbose: bool,
    silent: bool,
    buffer_size: u32,
    maildirs_dir: PathBuf,
    transformer_file: Option<PathBuf>,
    current_users: UserTracker,
}

impl InnerState {
    pub fn new(verbose: bool, silent: bool, buffer_size: u32, maildirs_dir: PathBuf, transformer_file: Option<PathBuf>) -> Self {
        Self {
            verbose,
            silent,
            buffer_size,
            maildirs_dir,
            transformer_file,
            current_users: UserTracker::new(),
        }
    }
}

#[derive(Clone, Copy)]
pub enum LoginUserError {
    AlreadyLoggedIn,
    WrongUserOrPass,
}

impl LoginUserError {
    pub const fn get_reason_str(self) -> &'static str {
        match self {
            Self::AlreadyLoggedIn => "User is already logged in",
            Self::WrongUserOrPass => "Wrong username or password",
        }
    }
}
