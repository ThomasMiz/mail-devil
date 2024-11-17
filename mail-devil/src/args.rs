//! A parser for the command-line arguments mail-devil can receive. The parser is invoked with the
//! `parse_arguments` function, which takes in an iterator of `String`s and returns a `Result` with
//! either an `ArgumentsRequest` on success, or an `ArgumentsError` on error.
//!
//! `ArgumentsRequest` is an enum with three variants; `Help`, `Version`, and
//! `Run(StartupArguments)`. This is to differentiate between when the user requests information to
//! the program, such as version or the help menu (and after displaying it the program should
//! close), or when the program should actually run a POP3 server, in which case that variant
//! provides a `StartupArguments` with the arguments parsed into a struct, including things like
//! the sockets to open, the path to the users file, which authentication methods are enabled, etc.
//! The `StartupArguments` instance is filled with default values for those not specified via
//! parameters.
//!
//! The `ArgumentsError` enum provides fine-detailed information on why the arguments are invalid.
//! This can include an unknown argument, as well as improper use of a valid argument. That said,
//! `ArgumentsError` as well as all subenums used within it implement the `fmt::Display` trait for
//! easy printing, so in order to print a human-readable explanation of why the syntax is invalid
//! a caller of `parse_arguments` may simply use `println!("{}", args_error);`.
//!
//! Additionally, the `get_version_string` and `get_help_string` functions provide human-readable
//! strings intended to be printed for their respective purposes.

use std::{
    collections::HashMap,
    fmt,
    io::ErrorKind,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, ToSocketAddrs},
};

use crate::pop3::Pop3ArgString;
use crate::util::buffer_size::{parse_pretty_buffer_size, PrettyBufferSizeParseError};

pub const DEFAULT_MAILDIRS_FILE: &str = "./maildirs";
pub const DEFAULT_POP3_PORT: u16 = 110;
pub const DEFAULT_BUFFER_SIZE: u32 = 0x2000;

pub fn get_version_string() -> String {
    format!(
        concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION"), " ({} {})"),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

pub fn get_help_string() -> &'static str {
    concat!(
        "Usage: mail-devil [options...]\n",
        "Options:\n",
        "  -h, --help                      Display this help menu and exit\n",
        "  -V, --version                   Display the version number and exit\n",
        "  -v, --verbose                   Display additional information while running\n",
        "  -s, --silent                    Do not print logs to stdout\n",
        "  -l, --listen <address>          Specify a socket address to listen for incoming POP3 clients\n",
        "  -d, --maildirs <path>           Specify the folder where to find the user's maildirs\n",
        "  -u, --user <user>               Adds a new user, or updates it if already present\n",
        "  -b, --buffer-size <size>        Sets the size of the buffer for client connections\n",
        "  -t, --transformer               Specifies a program to run for applying message transformations\n",
        "\n",
        "Socket addresses may be specified as an IPv4 or IPv6 address, or a domainname, and may include a port number. ",
        "The -l/--listen argument may be specified multiple times to listen on many addresses. If no port is specified, ",
        "then the default port of 110 will be used. If no -l/--listen argument is specified, then [::]:110 and ",
        "0.0.0.0:110 will be used.\n",
        "\n",
        "The maildirs directory, specified with -d/--maildirs, is where the user's maildirs are located. If, for ",
        "example, maildirs is \"./maildirs\" and there's a user named \"pablo\", then their emails will be stored in the ",
        "directory \"./maildirs/pablo\". The default maildirs directory is \"./maildirs\".\n",
        "\n",
        "Users are specified in a simple \"username:password\" format. The username may not contain a ':' character, and ",
        "all characters after the ':', including any ':' or trailing whitespaces, are considered part of the password. ",
        "The password for each user is stored in plaintext in a \"password\" file in their maildir directory. Due to POP3 ",
        "limitations, neither the username nor the password may exceed 40 bytes in length.\n",
        "\n",
        "The default buffer size is 8KBs. Buffer sizes may be specified in bytes ('-b 8192'), kilobytes ('-b 8K'), ",
        "megabytes ('-b 1M') or gigabytes ('-b 1G' if you respect your computer, please don't) but may not be equal to ",
        "nor larger than 4GBs.\n",
        "\n",
        "Programs for message transformation simply receive the Internet Message (RFC #822) on standard input and print ",
        "the processed message on standard output. If no transformer is specified, no transformation is applied. Only one ",
        "transformer may be specified.\n",
    )
}

#[derive(Debug, PartialEq)]
pub enum ArgumentsRequest {
    Help,
    Version,
    Run(StartupArguments),
}

#[derive(Debug, PartialEq)]
pub struct StartupArguments {
    pub pop3_bind_sockets: Vec<SocketAddr>,
    pub verbose: bool,
    pub silent: bool,
    pub maildirs_file: String,
    pub users: HashMap<Pop3ArgString, Pop3ArgString>,
    pub buffer_size: u32,
    pub transformer_file: String,
}

impl StartupArguments {
    pub fn empty() -> Self {
        StartupArguments {
            pop3_bind_sockets: Vec::new(),
            verbose: false,
            silent: false,
            maildirs_file: String::new(),
            users: HashMap::new(),
            buffer_size: 0,
            transformer_file: String::new(),
        }
    }

    pub fn fill_empty_fields_with_defaults(&mut self) {
        if self.pop3_bind_sockets.is_empty() {
            self.pop3_bind_sockets
                .push(SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, DEFAULT_POP3_PORT, 0, 0)));
            self.pop3_bind_sockets
                .push(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, DEFAULT_POP3_PORT)));
        }

        if self.maildirs_file.is_empty() {
            self.maildirs_file.push_str(DEFAULT_MAILDIRS_FILE);
        }

        if self.buffer_size == 0 {
            self.buffer_size = DEFAULT_BUFFER_SIZE;
        }
    }
}

impl Default for StartupArguments {
    fn default() -> Self {
        let mut args = Self::empty();
        args.fill_empty_fields_with_defaults();
        args
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ArgumentsError {
    UnknownArgument(String),
    Pop3ListenError(SocketErrorType),
    MaildirsFileError(FileErrorType),
    NewUserError(NewUserErrorType),
    BufferSizeError(BufferSizeErrorType),
    TransformerFileError(FileErrorType),
}

impl fmt::Display for ArgumentsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownArgument(arg) => write!(f, "Unknown argument: {arg}"),
            Self::Pop3ListenError(listen_error) => listen_error.fmt(f),
            Self::MaildirsFileError(users_file_error) => fmt_file_error_type(users_file_error, "users", f),
            Self::NewUserError(new_user_error) => new_user_error.fmt(f),
            Self::BufferSizeError(buffer_size_error) => buffer_size_error.fmt(f),
            Self::TransformerFileError(users_file_error) => fmt_file_error_type(users_file_error, "transformer", f),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum FileErrorType {
    UnexpectedEnd(String),
    AlreadySpecified(String),
    EmptyPath(String),
}

fn fmt_file_error_type(this: &FileErrorType, s: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match this {
        FileErrorType::UnexpectedEnd(arg) => write!(f, "Expected path to {s} file after {arg}"),
        FileErrorType::AlreadySpecified(_) => write!(f, "Only one {s} file may be specified"),
        FileErrorType::EmptyPath(arg) => write!(f, "Empty file name after {arg}"),
    }
}

fn parse_file_arg(result: &mut String, arg: String, maybe_arg2: Option<String>) -> Result<(), FileErrorType> {
    let arg2 = match maybe_arg2 {
        Some(arg2) => arg2,
        None => return Err(FileErrorType::UnexpectedEnd(arg)),
    };

    if arg2.is_empty() {
        return Err(FileErrorType::EmptyPath(arg));
    } else if !result.is_empty() {
        return Err(FileErrorType::AlreadySpecified(arg));
    }

    *result = arg2;
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub enum SocketErrorType {
    UnexpectedEnd(String),
    InvalidSocketAddress(String, String),
}

impl fmt::Display for SocketErrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEnd(arg) => write!(f, "Expected socket address after {arg}"),
            Self::InvalidSocketAddress(arg, addr) => write!(f, "Invalid socket address after {arg}: {addr}"),
        }
    }
}

fn parse_socket_arg(
    result_vec: &mut Vec<SocketAddr>,
    arg: String,
    maybe_arg2: Option<String>,
    default_port: u16,
) -> Result<(), SocketErrorType> {
    let arg2 = match maybe_arg2 {
        Some(value) => value,
        None => return Err(SocketErrorType::UnexpectedEnd(arg)),
    };

    let iter = match arg2.to_socket_addrs() {
        Ok(iter) => iter,
        Err(err) if err.kind() == ErrorKind::InvalidInput => match format!("{arg2}:{default_port}").to_socket_addrs() {
            Ok(iter) => iter,
            Err(_) => return Err(SocketErrorType::InvalidSocketAddress(arg, arg2)),
        },
        Err(_) => return Err(SocketErrorType::InvalidSocketAddress(arg, arg2)),
    };

    for sockaddr in iter {
        if !result_vec.contains(&sockaddr) {
            result_vec.push(sockaddr);
        }
    }

    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub enum NewUserErrorType {
    UnexpectedEnd(String),
    DuplicateUsername(String, String),
    UsernameTooLong(String, String),
    PasswordTooLong(String, String),
    InvalidUserSpecification(String, String),
}

impl fmt::Display for NewUserErrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEnd(arg) => write!(f, "Expected user specification after {arg}"),
            Self::DuplicateUsername(arg, arg2) => write!(f, "Duplicate username at {arg} {arg2}"),
            Self::UsernameTooLong(arg, arg2) => write!(f, "Username too long at {arg} {arg2}"),
            Self::PasswordTooLong(arg, arg2) => write!(f, "Password too long at {arg} {arg2}"),
            Self::InvalidUserSpecification(arg, arg2) => write!(f, "Invalid user specification at {arg} {arg2}"),
        }
    }
}

impl From<NewUserErrorType> for ArgumentsError {
    fn from(value: NewUserErrorType) -> Self {
        Self::NewUserError(value)
    }
}

fn parse_new_user_arg(result: &mut StartupArguments, arg: String, maybe_arg2: Option<String>) -> Result<(), NewUserErrorType> {
    let arg2 = match maybe_arg2 {
        Some(arg2) => arg2,
        None => return Err(NewUserErrorType::UnexpectedEnd(arg)),
    };

    // Trim any whitespaces at the start of `arg2` (not at the end, as to not trim the password).
    let arg2_trimmed = arg2.trim_ascii_start();

    // Find a ':' that is not at the end of the string, or otherwise return an invalid specification error.
    let colon_index = match arg2_trimmed.find(':') {
        Some(i) if i < arg2_trimmed.len() - 1 => i,
        _ => return Err(NewUserErrorType::InvalidUserSpecification(arg, arg2)),
    };

    let username_str = &arg2_trimmed[..colon_index];
    if username_str.len() > 40 {
        return Err(NewUserErrorType::UsernameTooLong(arg, arg2));
    }

    let password_str = &arg2_trimmed[(colon_index + 1)..];
    if password_str.len() > 40 {
        return Err(NewUserErrorType::PasswordTooLong(arg, arg2));
    }

    let username: Pop3ArgString = Pop3ArgString::from(username_str);
    let password: Pop3ArgString = Pop3ArgString::from(password_str);

    let vacant_entry = match result.users.entry(username) {
        std::collections::hash_map::Entry::Occupied(_) => return Err(NewUserErrorType::DuplicateUsername(arg, arg2)),
        std::collections::hash_map::Entry::Vacant(vac) => vac,
    };

    vacant_entry.insert(password);
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub enum BufferSizeErrorType {
    UnexpectedEnd(String),
    Empty(String),
    AlreadySpecified(String),
    CannotBeZero(String),
    InvalidFormat(String, String),
    InvalidCharacters(String, String),
    TooLarge(String, String),
}

impl fmt::Display for BufferSizeErrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEnd(arg) => write!(f, "Expected buffer size after {arg}"),
            Self::Empty(arg) => write!(f, "Empty buffer size argument after {arg}"),
            Self::AlreadySpecified(arg) => write!(f, "Buffer size already specified at {arg}"),
            Self::CannotBeZero(arg) => write!(f, "Buffer size cannot be zero at {arg}"),
            Self::InvalidFormat(arg, arg2) => write!(f, "Invalid buffer size format at {arg} {arg2}"),
            Self::InvalidCharacters(arg, arg2) => write!(f, "Buffer size contains invalid characters at {arg} {arg2}"),
            Self::TooLarge(arg, arg2) => write!(f, "Buffer size must be less than 4GB at {arg} {arg2}"),
        }
    }
}

impl From<BufferSizeErrorType> for ArgumentsError {
    fn from(value: BufferSizeErrorType) -> Self {
        Self::BufferSizeError(value)
    }
}

fn parse_buffer_size_arg(result: &mut StartupArguments, arg: String, maybe_arg2: Option<String>) -> Result<(), BufferSizeErrorType> {
    let arg2 = match maybe_arg2 {
        Some(arg2) => arg2,
        None => return Err(BufferSizeErrorType::UnexpectedEnd(arg)),
    };

    if result.buffer_size != 0 {
        return Err(BufferSizeErrorType::AlreadySpecified(arg));
    }

    let size = match parse_pretty_buffer_size(&arg2) {
        Ok(s) => s,
        Err(parse_error) => {
            return Err(match parse_error {
                PrettyBufferSizeParseError::Empty => BufferSizeErrorType::Empty(arg),
                PrettyBufferSizeParseError::Zero => BufferSizeErrorType::CannotBeZero(arg),
                PrettyBufferSizeParseError::InvalidFormat => BufferSizeErrorType::InvalidFormat(arg, arg2),
                PrettyBufferSizeParseError::InvalidCharacters => BufferSizeErrorType::InvalidCharacters(arg, arg2),
                PrettyBufferSizeParseError::TooLarge => BufferSizeErrorType::TooLarge(arg, arg2),
            })
        }
    };

    result.buffer_size = size;
    Ok(())
}

pub fn parse_arguments<T>(mut args: T) -> Result<ArgumentsRequest, ArgumentsError>
where
    T: Iterator<Item = String>,
{
    let mut result = StartupArguments::empty();

    // Ignore the first argument, as it's by convention the name of the program
    args.next();

    while let Some(arg) = args.next() {
        if arg.is_empty() {
            continue;
        } else if arg.eq("-h") || arg.eq_ignore_ascii_case("--help") {
            return Ok(ArgumentsRequest::Help);
        } else if arg.eq("-V") || arg.eq_ignore_ascii_case("--version") {
            return Ok(ArgumentsRequest::Version);
        } else if arg.eq("-v") || arg.eq_ignore_ascii_case("--verbose") {
            result.verbose = true;
        } else if arg.eq("-s") || arg.eq_ignore_ascii_case("--silent") {
            result.silent = true;
        } else if arg.eq("-l") || arg.eq_ignore_ascii_case("--listen") {
            parse_socket_arg(&mut result.pop3_bind_sockets, arg, args.next(), DEFAULT_POP3_PORT)
                .map_err(ArgumentsError::Pop3ListenError)?;
        } else if arg.eq("-d") || arg.eq_ignore_ascii_case("--maildirs") {
            parse_file_arg(&mut result.maildirs_file, arg, args.next()).map_err(ArgumentsError::MaildirsFileError)?;
        } else if arg.eq("-u") || arg.eq_ignore_ascii_case("--user") {
            parse_new_user_arg(&mut result, arg, args.next())?;
        } else if arg.eq("-b") || arg.eq_ignore_ascii_case("--buffer-size") {
            parse_buffer_size_arg(&mut result, arg, args.next())?;
        } else if arg.eq("-t") || arg.eq_ignore_ascii_case("--transformer") {
            parse_file_arg(&mut result.transformer_file, arg, args.next()).map_err(ArgumentsError::TransformerFileError)?;
        } else {
            return Err(ArgumentsError::UnknownArgument(arg));
        }
    }

    result.fill_empty_fields_with_defaults();
    Ok(ArgumentsRequest::Run(result))
}
