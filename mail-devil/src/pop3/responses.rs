use std::{
    fmt::{self, Display, Write},
    io,
};

use inlined::TinyString;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::types::{MessageNumber, MessageNumberCount};

/// The value allowed by the RFC is 512, but we don't need that much. This includes the `+OK` or `-ERR` and the `CRLF`.
pub const MAX_RESPONSE_LENGTH: usize = 100;

/// Represents a POP3 single-line response. Use [`Pop3Response::write_to`] to write it to a buffer.
pub enum Pop3Response<T: Display, E: Display> {
    Ok(Option<T>),
    Err(Option<E>),
}

impl<T: Display, E: Display> Pop3Response<T, E> {
    /// Writes a POP3 response into the given possibly-unbuffered writer with an optional message.
    ///
    /// Since the writer may be unbuffered, this function uses a small inline buffer of size [`MAX_RESPONSE_LENGTH`] to
    /// buffer the contents and write them all at once.
    ///
    /// Writes `+OK` or `-ERR` depending on whether `status` is `true` or `false`, and then if `message` is [`Some`],
    /// appends a space followed by the given message. The message may be any type that implements the [`Display`] trait.
    pub async fn write_to<W>(&self, writer: &mut W) -> io::Result<()>
    where
        W: AsyncWrite + Unpin + ?Sized,
    {
        let mut buf: TinyString<MAX_RESPONSE_LENGTH> = TinyString::new();

        match self {
            Self::Ok(maybe_msg) => {
                buf.push_str("+OK");
                if let Some(msg) = maybe_msg {
                    let _ = write!(buf, " {msg}");
                }
            }
            Self::Err(maybe_msg) => {
                buf.push_str("-ERR");
                if let Some(msg) = maybe_msg {
                    let _ = write!(buf, " {msg}");
                }
            }
        }

        // Make space for two characters at the end for the CRLF
        while buf.len() > buf.capacity() - 2 {
            buf.pop();
        }

        buf.push_str("\r\n");
        writer.write_all(buf.as_bytes()).await
    }
}

impl<T: Display> Pop3Response<T, &str> {
    pub const fn ok(message: T) -> Self {
        Self::Ok(Some(message))
    }
}

impl Pop3Response<&str, &str> {
    pub const fn ok_empty() -> Self {
        Self::Ok(None)
    }
}

impl<E: Display> Pop3Response<&str, E> {
    pub const fn err(message: E) -> Self {
        Self::Err(Some(message))
    }
}

pub struct TwoNumDisplay {
    pub first: MessageNumberCount,
    pub second: u64,
}

impl TwoNumDisplay {
    pub const fn new(first: MessageNumberCount, second: u64) -> Self {
        Self { first, second }
    }
}

impl Display for TwoNumDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.first, self.second)
    }
}

impl<E: Display> Pop3Response<TwoNumDisplay, E> {
    pub const fn ok_stat(message_count: MessageNumberCount, maildrop_size: u64) -> Self {
        Self::Ok(Some(TwoNumDisplay::new(message_count, maildrop_size)))
    }
}

impl Pop3Response<TwoNumDisplay, &str> {
    pub const fn ok_list_one(message_number: MessageNumber, message_size: u64) -> Self {
        Self::Ok(Some(TwoNumDisplay::new(message_number.get(), message_size)))
    }
}

pub struct MessagesDeletedDisplay {
    pub count: MessageNumberCount,
}

impl MessagesDeletedDisplay {
    pub const fn new(count: MessageNumberCount) -> Self {
        Self { count }
    }
}

impl Display for MessagesDeletedDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} messages deleted", self.count)
    }
}

impl<E: Display> Pop3Response<MessagesDeletedDisplay, E> {
    pub const fn ok_deleted(count: MessageNumberCount) -> Self {
        Self::Ok(Some(MessagesDeletedDisplay::new(count)))
    }
}

impl<T: Display> Pop3Response<T, MessagesDeletedDisplay> {
    pub const fn err_deleted(count: MessageNumberCount) -> Self {
        Self::Err(Some(MessagesDeletedDisplay::new(count)))
    }
}
