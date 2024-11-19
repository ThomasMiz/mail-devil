use std::{
    fmt::{self, Display, Write},
    io,
};

use inlined::TinyString;
use tokio::io::{AsyncWrite, AsyncWriteExt};

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

    pub const fn ok_empty() -> Self {
        Self::Ok(None)
    }
}

impl<E: Display> Pop3Response<&str, E> {
    pub const fn err(message: E) -> Self {
        Self::Err(Some(message))
    }
}

impl<E: Display> Pop3Response<StatDisplay, E> {
    pub const fn ok_stat(message_count: usize, maildrop_size: u64) -> Self {
        Self::Ok(Some(StatDisplay::new(message_count, maildrop_size)))
    }
}

pub struct StatDisplay {
    pub message_count: usize,
    pub maildrop_size: u64,
}

impl StatDisplay {
    pub const fn new(message_count: usize, maildrop_size: u64) -> Self {
        Self {
            message_count,
            maildrop_size,
        }
    }
}

impl Display for StatDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.message_count, self.maildrop_size)
    }
}
