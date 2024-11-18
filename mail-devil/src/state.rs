// TODO

use std::{cell::RefCell, collections::HashSet, path::PathBuf, rc::Rc};

use inlined::TinyString;

use crate::user_tracker::UserTracker;

/// Stores the POP3 server's state.
///
/// This is a reference type which may be cloned to create multiple references to the same state.
#[derive(Clone)]
pub struct Pop3ServerState {
    rc: Rc<InnerState>,
}

impl Pop3ServerState {
    pub fn new(verbose: bool, silent: bool, maildirs_dir: PathBuf, transformer_file: Option<PathBuf>) -> Self {
        Self {
            rc: Rc::new(InnerState::new(verbose, silent, maildirs_dir, transformer_file)),
        }
    }

    pub fn verbose(&self) -> bool {
        self.rc.verbose
    }

    pub fn silent(&self) -> bool {
        self.rc.silent
    }
}

/// Stores the immutable variables of a POP3 server's state.
struct InnerState {
    verbose: bool,
    silent: bool,
    maildirs_dir: PathBuf,
    transformer_file: Option<PathBuf>,
    current_users: UserTracker,
}

impl InnerState {
    pub fn new(verbose: bool, silent: bool, maildirs_dir: PathBuf, transformer_file: Option<PathBuf>) -> Self {
        Self {
            verbose,
            silent,
            maildirs_dir,
            transformer_file,
            current_users: UserTracker::new(),
        }
    }
}
