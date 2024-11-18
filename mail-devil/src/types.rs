use std::{fmt, num::NonZeroU16, ops::Deref};

use inlined::TinyString;

use crate::util::ascii::IsValidUsername;

/// The name of the file containing the plaintext password within each user's maildrop directory.
pub const PASSWORD_FILE_NAME: &str = "password";

/// The maximum allowed length (in bytes) for a POP3 command argument (taken from RFC #1939).
pub const MAX_COMMAND_ARG_LENGTH: usize = 40;

pub type Pop3ArgString = TinyString<MAX_COMMAND_ARG_LENGTH>;
pub type MessageNumber = NonZeroU16;

#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pop3Username(Pop3ArgString);

pub struct NonValidUsernameError;

/*impl<R: AsRef<str>> TryFrom<R> for Pop3Username {
    type Error = NonValidUsernameError;

    fn try_from(value: &R) -> Result<Self, Self::Error> {
        let s = value.as_ref();
        match s.is_valid_username() {
            true => Ok(Pop3Username(Pop3ArgString::from(s))),
            false => Err(NonValidUsernameError),
        }
    }
}*/
impl TryFrom<&str> for Pop3Username {
    type Error = NonValidUsernameError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.is_valid_username() {
            true => Ok(Pop3Username(Pop3ArgString::from(value))),
            false => Err(NonValidUsernameError),
        }
    }
}

impl Deref for Pop3Username {
    type Target = Pop3ArgString;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for Pop3Username {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Debug for Pop3Username {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}