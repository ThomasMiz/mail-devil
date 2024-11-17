use std::{
    io::{self, ErrorKind},
    num::NonZeroU16,
    str::FromStr,
};

use inlined::{TinyString, TinyVec};
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

use crate::util::ascii;

/// The maximum allowed length (in bytes) for a single line with a POP3 command.
pub const MAX_COMMAND_LINE_LENGTH: usize = 255;

/// The maximum allowed length (in bytes) for a POP3 command argument (taken from RFC #1939).
pub const MAX_COMMAND_ARG_LENGTH: usize = 40;

// All command keywords are 4 bytes, so for easier comparison we represent them as little-endian int32s in uppercase.
const USER_COMMAND_CODE: u32 = u32::from_le_bytes([b'U', b'S', b'E', b'R']);
const PASS_COMMAND_CODE: u32 = u32::from_le_bytes([b'P', b'A', b'S', b'S']);
const STAT_COMMAND_CODE: u32 = u32::from_le_bytes([b'S', b'T', b'A', b'T']);
const LIST_COMMAND_CODE: u32 = u32::from_le_bytes([b'L', b'I', b'S', b'T']);
const RETR_COMMAND_CODE: u32 = u32::from_le_bytes([b'R', b'E', b'T', b'R']);
const DELE_COMMAND_CODE: u32 = u32::from_le_bytes([b'D', b'E', b'L', b'E']);
const NOOP_COMMAND_CODE: u32 = u32::from_le_bytes([b'N', b'O', b'O', b'P']);
const RSET_COMMAND_CODE: u32 = u32::from_le_bytes([b'R', b'S', b'E', b'T']);
const QUIT_COMMAND_CODE: u32 = u32::from_le_bytes([b'Q', b'U', b'I', b'T']);

#[derive(Debug)]
pub enum Pop3Command {
    User(TinyString<MAX_COMMAND_ARG_LENGTH>),
    Pass(TinyString<MAX_COMMAND_ARG_LENGTH>),
    Quit,
    Stat,
    List(Option<NonZeroU16>),
    Retr(NonZeroU16),
    Dele(NonZeroU16),
    Noop,
    Rset,
}

#[derive(Debug)]
pub enum Pop3CommandError {
    EmptyLine,
    UnknownCommand,
    NonPrintableAsciiChar(u8),
    User(UserCommandError),
    Pass(PassCommandError),
    Quit(NoArgCommandError),
    Stat(NoArgCommandError),
    List(OptionalNumericArgError),
    Retr(NumericArgCommandError),
    Dele(NumericArgCommandError),
    Noop(NoArgCommandError),
    Rset(NoArgCommandError),
    IO(io::Error),
}

impl From<io::Error> for Pop3CommandError {
    fn from(value: io::Error) -> Self {
        Self::IO(value)
    }
}

#[derive(Debug)]
pub enum UserCommandError {
    NoArguments,
    TooManyArguments,
    ArgumentTooLong,
}

impl From<UserCommandError> for Pop3CommandError {
    fn from(value: UserCommandError) -> Self {
        Self::User(value)
    }
}

#[derive(Debug)]
pub enum PassCommandError {
    NoArgument,
    ArgumentTooLong,
}

impl From<PassCommandError> for Pop3CommandError {
    fn from(value: PassCommandError) -> Self {
        Self::Pass(value)
    }
}

#[derive(Debug)]
pub enum NoArgCommandError {
    TooManyArguments,
}

#[derive(Debug)]
pub enum OptionalNumericArgError {
    TooManyArguments,
    InvalidArgument,
}

#[derive(Debug)]
pub enum NumericArgCommandError {
    NoArgument,
    TooManyArguments,
    InvalidArgument,
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

    if buf.is_empty() {
        return Err(Pop3CommandError::EmptyLine);
    }

    // Check that the whole line consists only of printable ASCII characters and if not, return an appropriate error.
    let _ = ascii::printable_ascii_from_bytes(&buf).map_err(Pop3CommandError::NonPrintableAsciiChar)?;

    // All the arguments implemented in this server are exactly 4 chars long, let's ensure that here for easy parsing.
    if buf.len() < 4 || (buf.len() > 4 && !buf[4].is_ascii_whitespace()) {
        return Err(Pop3CommandError::UnknownCommand);
    }

    // Calculate the command's "code", which is done by interpreting the uppercased chars as a little-endian u32.
    buf[..4].make_ascii_uppercase();
    let command = <[u8; 4]>::try_from(&buf[..4]).unwrap();
    let command_code = u32::from_le_bytes(command);

    // Get the remaining arguments as a single string, stripping the space after the command, or an empty string.
    let args = match buf.len() >= 6 {
        true => unsafe { std::str::from_utf8_unchecked(&buf[5..]) },
        false => "",
    };

    match command_code {
        USER_COMMAND_CODE => Ok(Pop3Command::User(parse_user_command(args)?)),
        PASS_COMMAND_CODE => Ok(Pop3Command::Pass(parse_pass_command(args)?)),
        QUIT_COMMAND_CODE => parse_no_arg_command(args, Pop3Command::Quit).map_err(Pop3CommandError::Quit),
        STAT_COMMAND_CODE => parse_no_arg_command(args, Pop3Command::Stat).map_err(Pop3CommandError::Stat),
        LIST_COMMAND_CODE => Ok(Pop3Command::List(parse_optnum_command(args).map_err(Pop3CommandError::List)?)),
        RETR_COMMAND_CODE => Ok(Pop3Command::Retr(parse_num_command(args).map_err(Pop3CommandError::Retr)?)),
        DELE_COMMAND_CODE => Ok(Pop3Command::Dele(parse_num_command(args).map_err(Pop3CommandError::Dele)?)),
        NOOP_COMMAND_CODE => parse_no_arg_command(args, Pop3Command::Noop).map_err(Pop3CommandError::Noop),
        RSET_COMMAND_CODE => parse_no_arg_command(args, Pop3Command::Rset).map_err(Pop3CommandError::Rset),
        _ => Err(Pop3CommandError::UnknownCommand),
    }
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
            reader.consume(consumed_bytes);
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

fn parse_user_command(args: &str) -> Result<TinyString<MAX_COMMAND_ARG_LENGTH>, UserCommandError> {
    let mut split = args.trim().split_ascii_whitespace();

    match split.next() {
        None => Err(UserCommandError::NoArguments),
        Some(username) if username.len() > 40 => Err(UserCommandError::ArgumentTooLong),
        Some(_) if split.next().is_some() => Err(UserCommandError::TooManyArguments),
        Some(username) => Ok(TinyString::from(username)),
    }
}

fn parse_pass_command(args: &str) -> Result<TinyString<MAX_COMMAND_ARG_LENGTH>, PassCommandError> {
    if args.is_empty() {
        return Err(PassCommandError::NoArgument);
    }

    if args.len() > 40 {
        return Err(PassCommandError::ArgumentTooLong);
    }

    Ok(TinyString::from(args))
}

fn parse_no_arg_command(args: &str, command: Pop3Command) -> Result<Pop3Command, NoArgCommandError> {
    if !args.trim().is_empty() {
        return Err(NoArgCommandError::TooManyArguments);
    }

    Ok(command)
}

fn parse_optnum_command(args: &str) -> Result<Option<NonZeroU16>, OptionalNumericArgError> {
    let mut split = args.trim().split_ascii_whitespace();

    match split.next().map(NonZeroU16::from_str) {
        None => Ok(None),
        Some(_) if split.next().is_some() => Err(OptionalNumericArgError::TooManyArguments),
        Some(Ok(number)) => Ok(Some(number)),
        Some(Err(_)) => Err(OptionalNumericArgError::InvalidArgument),
    }
}

fn parse_num_command(args: &str) -> Result<NonZeroU16, NumericArgCommandError> {
    let mut split = args.trim().split_ascii_whitespace();

    match split.next().map(NonZeroU16::from_str) {
        None => Err(NumericArgCommandError::NoArgument),
        Some(_) if split.next().is_some() => Err(NumericArgCommandError::TooManyArguments),
        Some(Ok(number)) => Ok(number),
        Some(Err(_)) => Err(NumericArgCommandError::InvalidArgument),
    }
}
