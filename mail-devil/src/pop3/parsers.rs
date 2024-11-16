use std::io::{self, ErrorKind};

use inlined::{TinyString, TinyVec};
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

use crate::util::ascii;

/// The maximum allowed length (in bytes) for a single line with a POP3 command.
pub const MAX_COMMAND_LINE_LENGTH: usize = 255;

/// The maximum allowed length (in bytes) for a POP3 command argument (taken from RFC #1939).
pub const MAX_COMMAND_ARG_LENGTH: usize = 40;

pub enum Pop3Command {
    User(TinyString<MAX_COMMAND_ARG_LENGTH>),
    Pass(TinyString<MAX_COMMAND_ARG_LENGTH>),
}

pub enum Pop3CommandError {
    UnknownCommand,
    NonPrintableAsciiChar(u8),
    ArgumentTooLong,
    IO(io::Error),
}

impl From<io::Error> for Pop3CommandError {
    fn from(value: io::Error) -> Self {
        Pop3CommandError::IO(value)
    }
}

/// Reads a POP3 command from the given buffered reader, reading up to (and including) the next end of line, and
/// parses the command into a [`Pop3Command`] struct.
///
/// Returns [`Ok`] with the parsed command on success. Or otherwise, [`Err`] with error that occurred.
pub async fn parse_command<R>(reader: &mut R) -> Result<Pop3Command, Pop3CommandError>
where
    R: AsyncBufRead + Unpin + ?Sized,
{
    // An inlined buffer into which we will copy an entire line before parsing it all at once.
    let mut buf: TinyVec<MAX_COMMAND_LINE_LENGTH, u8> = TinyVec::new();

    // Fill `buf` with a new line, checking for errors along the way.
    read_line(reader, &mut buf).await?;

    // Check that the whole line consists only of printable ASCII characters and if not, return an appropriate error.
    let line = ascii::printable_ascii_from_bytes(&buf).map_err(Pop3CommandError::NonPrintableAsciiChar)?;
    println!("Aight, here's the {}-byte line received: {line}", line.len());

    Err(Pop3CommandError::ArgumentTooLong)
}

/// Reads a line from the given reader and appends it to the given `TinyVec`. Supports both CRLF and LF, and in both
/// cases the newline sequence is not appended to the buffer.
///
/// Returns [`Ok`] if a whole line was successfully read, or [`Err`] if an IO error occurred while reading from the
/// reader.
///
/// The only case in which this function does not read up to the end of the line is when the line is longer than `buf`
/// can hold, in which case [`Err`] with a custom [`io::Error`] is returned with kind [`ErrorKind::InvalidData`].
async fn read_line<const N: usize, R>(reader: &mut R, buf: &mut TinyVec<N, u8>) -> io::Result<()>
where
    R: AsyncBufRead + Unpin + ?Sized,
{
    loop {
        // Wait for the reader to have bytes available and grab up to `buf_remaining_capacity` of them.
        let reader_buf = reader.fill_buf().await?;
        let buf_remaining_capacity = buf.capacity() - buf.len();
        let reader_buf = &reader_buf[..reader_buf.len().min(buf_remaining_capacity as usize + 1)];

        // Look for an end of line within the newly read bytes (we look for '\n' and later handle the '\r').
        let mut maybe_line_end_index = reader_buf.iter().position(|b| *b == b'\n');
        let consumed_bytes = maybe_line_end_index.map(|b| b + 1).unwrap_or(reader_buf.len());

        // If a '\n' was found and it's not at the beginning of the line, remove the preceding '\r' if present.
        if let Some(line_end_index) = &mut maybe_line_end_index {
            if *line_end_index != 0 && reader_buf[*line_end_index - 1] == b'\r' {
                *line_end_index -= 1;
            }
        }

        // Calculate how many new bytes we have to append to `buf` (it's either up to the CRLF, or the whole buffer).
        let new_byte_count = maybe_line_end_index.unwrap_or(reader_buf.len());

        // If there's no end of line and the buffer is overfilled, then the line is over the maximum length.
        if maybe_line_end_index.is_none() && buf.len() as usize + new_byte_count >= buf.capacity() as usize {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                format!("POP3 lines must be at most {N} characters long"),
            ));
        }

        // Copy the new bytes from `reader_buf` to `buf`, then mark the bytes up to the '\n' as consumed.
        buf.extend_from_slice_copied(&reader_buf[..new_byte_count]);
        reader.consume(consumed_bytes);

        if maybe_line_end_index.is_some() {
            return Ok(());
        }
    }
}
