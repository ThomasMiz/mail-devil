//! Provides methods for reading and parsing POP3 commands (client requests).
//!
//! The intended usage of this module is to first use the asynchronous [`read_line`] to read an entire line, and then
//! pass this line into the synchronous [`parse_command`]. The reason for not including all this behavior into a single
//! method is to allow keeping the line buffer outside of the parser, and thus allowing it to be cancel safe.

use std::{
    fmt,
    io::{self, ErrorKind},
    str::FromStr,
};

use inlined::TinyVec;
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

use crate::{
    types::{MessageNumber, Pop3ArgString, Pop3Username},
    util::ascii,
};

/// The maximum allowed length (in bytes) for a single line with a POP3 command.
pub const MAX_COMMAND_LINE_LENGTH: usize = 255;

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
    User(Pop3Username),
    Pass(Pop3ArgString),
    Quit,
    Stat,
    List(Option<MessageNumber>),
    Retr(MessageNumber),
    Dele(MessageNumber),
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
}

impl fmt::Display for Pop3CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyLine => write!(f, "How about you try actually writing something? Dumbass"),
            Self::UnknownCommand => write!(f, "Unknown command"),
            Self::NonPrintableAsciiChar(c) => write!(f, "Non ASCII character with code 0x{c:x}"),
            Self::User(e) => e.fmt(f),
            Self::Pass(e) => e.fmt(f),
            Self::Quit(e) => e.fmt(f),
            Self::Stat(e) => e.fmt(f),
            Self::List(e) => e.fmt(f),
            Self::Retr(e) => e.fmt(f),
            Self::Dele(e) => e.fmt(f),
            Self::Noop(e) => e.fmt(f),
            Self::Rset(e) => e.fmt(f),
        }
    }
}

#[derive(Debug)]
pub enum UserCommandError {
    NoArguments,
    TooManyArguments,
    ArgumentTooLong,
    InvalidUsername,
}

impl fmt::Display for UserCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoArguments => write!(f, "No username specified"),
            Self::TooManyArguments => write!(f, "Too many arguments"),
            Self::ArgumentTooLong => write!(f, "Usernames must be at most 40 characters long"),
            Self::InvalidUsername => write!(f, "Username contains invalid characters"),
        }
    }
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

impl fmt::Display for PassCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoArgument => write!(f, "No password specified"),
            Self::ArgumentTooLong => write!(f, "Passwords must be at most 40 characters long"),
        }
    }
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

impl fmt::Display for NoArgCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "This command takes no arguments")
    }
}

#[derive(Debug)]
pub enum OptionalNumericArgError {
    TooManyArguments,
    InvalidArgument,
}

impl fmt::Display for OptionalNumericArgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyArguments => write!(f, "This command takes at most one argument"),
            Self::InvalidArgument => write!(f, "Argument is not a valid number"),
        }
    }
}

#[derive(Debug)]
pub enum NumericArgCommandError {
    NoArgument,
    TooManyArguments,
    InvalidArgument,
}

impl fmt::Display for NumericArgCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoArgument => write!(f, "This command takes exactly one argument"),
            Self::TooManyArguments => write!(f, "Too many arguments"),
            Self::InvalidArgument => write!(f, "Argument is not a valid number"),
        }
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
///
/// # Cancel safety
/// This method might have read some data from the reader and appended it to `buf` before completing. However, if used
/// in a loop with multiple branches, canceling this method and then calling it again with the same buffer will yield
/// the same end result as if the method was allowed to run to the end in a single call, and thus in such a use case it
/// is considered cancel safe.
pub async fn read_line<const N: usize, R>(reader: &mut R, buf: &mut TinyVec<N, u8>) -> io::Result<()>
where
    R: AsyncBufRead + Unpin + ?Sized,
{
    loop {
        // Wait for the reader to have bytes available and grab up to `buf_remaining_capacity` of them.
        let reader_buf = reader.fill_buf().await?;
        if reader_buf.is_empty() {
            return Err(io::Error::from(ErrorKind::UnexpectedEof));
        }

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

/// Parses a POP3 command from the given buffer, which is intended to contain exactly one line without the line ending
/// sequence.
///
/// This a synchronous method. The intended usage is to first use something like [`read_line`] to read an entire line
/// from an asynchronous reader, and then pass the whole line to this parser.
///
/// Returns [`Ok`] with the parsed command on success. Or otherwise, [`Err`] with error that occurred.
pub fn parse_command(buf: &mut [u8]) -> Result<Pop3Command, Pop3CommandError> {
    if buf.is_empty() {
        return Err(Pop3CommandError::EmptyLine);
    }

    // Check that the whole line consists only of printable ASCII characters and if not, return an appropriate error.
    let _ = ascii::printable_ascii_from_bytes(buf).map_err(Pop3CommandError::NonPrintableAsciiChar)?;

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

fn parse_user_command(args: &str) -> Result<Pop3Username, UserCommandError> {
    let mut split = args.trim().split_ascii_whitespace();

    match split.next() {
        None => Err(UserCommandError::NoArguments),
        Some(username) if username.len() > 40 => Err(UserCommandError::ArgumentTooLong),
        Some(_) if split.next().is_some() => Err(UserCommandError::TooManyArguments),
        Some(username) => Pop3Username::try_from(username).map_err(|_| UserCommandError::InvalidUsername),
    }
}

fn parse_pass_command(args: &str) -> Result<Pop3ArgString, PassCommandError> {
    if args.is_empty() {
        return Err(PassCommandError::NoArgument);
    }

    if args.len() > 40 {
        return Err(PassCommandError::ArgumentTooLong);
    }

    Ok(Pop3ArgString::from(args))
}

fn parse_no_arg_command(args: &str, command: Pop3Command) -> Result<Pop3Command, NoArgCommandError> {
    if !args.trim().is_empty() {
        return Err(NoArgCommandError::TooManyArguments);
    }

    Ok(command)
}

fn parse_optnum_command(args: &str) -> Result<Option<MessageNumber>, OptionalNumericArgError> {
    let mut split = args.trim().split_ascii_whitespace();

    match split.next().map(MessageNumber::from_str) {
        None => Ok(None),
        Some(_) if split.next().is_some() => Err(OptionalNumericArgError::TooManyArguments),
        Some(Ok(number)) => Ok(Some(number)),
        Some(Err(_)) => Err(OptionalNumericArgError::InvalidArgument),
    }
}

fn parse_num_command(args: &str) -> Result<MessageNumber, NumericArgCommandError> {
    let mut split = args.trim().split_ascii_whitespace();

    match split.next().map(MessageNumber::from_str) {
        None => Err(NumericArgCommandError::NoArgument),
        Some(_) if split.next().is_some() => Err(NumericArgCommandError::TooManyArguments),
        Some(Ok(number)) => Ok(number),
        Some(Err(_)) => Err(NumericArgCommandError::InvalidArgument),
    }
}
