use std::{
    fmt::{Display, Write},
    io,
};

use inlined::TinyString;
use tokio::io::{AsyncWrite, AsyncWriteExt};

/// The value allowed by the RFC is 512, but we don't need that much. This includes the `+OK` or `-ERR` and the `CRLF`.
pub const MAX_RESPONSE_LENGTH: usize = 100;

/// Represents a POP3 single-line response.
pub enum Pop3Response<T: Display> {
    Ok(Option<T>),
    Err(Option<T>),
}

impl<T: Display> Pop3Response<T> {
    pub async fn write_to<W>(&self, writer: &mut W) -> io::Result<()>
    where
        W: AsyncWrite + Unpin + ?Sized,
    {
        let (status, message) = match self {
            Self::Ok(msg) => (true, msg.as_ref()),
            Self::Err(msg) => (false, msg.as_ref()),
        };

        write_response(writer, status, message).await
    }
}

/// Writes a POP3 response into the given possibly-unbuffered writer with an optional message.
///
/// Since the writer may be unbuffered, this function uses a small inline buffer of size [`MAX_RESPONSE_LENGTH`] to
/// buffer the contents and write them all at once.
///
/// Writes `+OK` or `-ERR` depending on whether `status` is `true` or `false`, and then if `message` is [`Some`],
/// appends a space followed by the given message. The message may be any type that implements the [`Display`] trait.
pub async fn write_response<W, T>(writer: &mut W, status: bool, message: Option<T>) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
    T: Display,
{
    let mut buf: TinyString<MAX_RESPONSE_LENGTH> = TinyString::new();

    buf.push_str(match status {
        true => "+OK",
        false => "-ERR",
    });

    if let Some(msg) = message {
        let _ = write!(buf, " {msg}");
    }

    // Make space for two characters at the end for the CRLF
    while buf.len() > buf.capacity() - 2 {
        buf.pop();
    }

    buf.push_str("\r\n");
    writer.write_all(buf.as_bytes()).await
}

/// Writes a POP3 response into the given possibly-unbuffered writer without an additional message.
///
/// Works the same way as [`write_response`].
pub async fn write_empty_response<W>(writer: &mut W, status: bool) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    write_response(writer, status, None::<&str>).await
}
