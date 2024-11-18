//! A set of utility methods for working with ASCII strings.

use crate::types::MAX_COMMAND_ARG_LENGTH;

/// A simple trait for checking whether a type is an ASCII printable character.
pub trait IsPrintableAscii {
    /// Returns whether this is a printable ASCII character.
    fn is_printable_ascii(&self) -> bool;
}

impl IsPrintableAscii for u8 {
    fn is_printable_ascii(&self) -> bool {
        *self >= b' ' && *self <= b'~'
    }
}

/// Checks that the given byte slice is composed of only ASCII chars and if so, returns [`Ok`] with the same slice as a
/// `&str`.
///
/// If the byte slice is not ASCII, then [`Err`] is returned with the first occurrance of a non-ASCII byte.
pub fn printable_ascii_from_bytes(buf: &[u8]) -> Result<&str, u8> {
    if let Some(offending_byte) = buf.iter().copied().find(|b| !b.is_printable_ascii()) {
        return Err(offending_byte);
    }

    // SAFETY: We previously ensured `buf` contains only ASCII chars, and thus it is UTF-8.
    Ok(unsafe { std::str::from_utf8_unchecked(buf) })
}

/// A simple trait for checking whether a type is a valid username.
pub trait IsValidUsername {
    fn is_valid_username(&self) -> bool;
}

impl IsValidUsername for [u8] {
    fn is_valid_username(&self) -> bool {
        if self.len() == 0 || self.len() > MAX_COMMAND_ARG_LENGTH {
            return false;
        }

        // Must start with a-z A-Z or '_'.
        if (self[0] < b'a' || self[0] > b'z') && (self[0] < b'A' || self[0] > b'Z') && self[0] != b'_' {
            return false;
        }

        // All other characters may be a-z, A-Z, 0-9 or '_'.
        self.iter()
            .copied()
            .all(|b| (b >= b'0' && b <= b'9') || (b >= b'a' && b <= b'z') || b == b'_')
    }
}

impl IsValidUsername for str {
    fn is_valid_username(&self) -> bool {
        self.as_bytes().is_valid_username()
    }
}
