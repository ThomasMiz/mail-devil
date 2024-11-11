use std::io::{self, ErrorKind};

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
};

pub async fn handle_client(mut socket: TcpStream) -> io::Result<()> {
    let (read_half, mut write_half) = socket.split();
    let reader = BufReader::new(read_half);

    write_half.write_all(b"+OK No swearing on my christian POP3 server\r\n").await?;

    let mut lines = reader.lines();

    let mut username = String::new();
    let mut password = String::new();
    while username.is_empty() || password.is_empty() {
        let line = match lines.next_line().await? {
            Some(s) => s,
            None => continue,
        };

        let mut split = line.split_ascii_whitespace();
        match (split.next(), split.next()) {
            (Some(cmd), Some(user_arg)) if cmd.eq_ignore_ascii_case("USER") => {
                username.clear();
                username.push_str(user_arg);
                write_half.write_all(b"+OK gotchu bro\r\n").await?;
            }
            (Some(cmd), Some(pass_arg)) if cmd.eq_ignore_ascii_case("PASS") => {
                if username.is_empty() {
                    write_half.write_all(b"-ERR 'kay bro but I dunno who u are\r\n").await?;
                } else {
                    password.clear();
                    password.push_str(pass_arg);
                }
            }
            (Some(cmd), _) if cmd.eq_ignore_ascii_case("QUIT") => {
                write_half.write_all(b"-ERR Leavin' already??\r\n").await?;
                socket.shutdown().await?;
                return Ok(());
            }
            _ => {
                return Err(io::Error::new(
                    ErrorKind::Other,
                    "Client is not following the god damn protocol, yeet",
                ))
            }
        }
    }

    println!("Deez mf logged in as {username} {password}");
    write_half.write_all(b"+OK Every Friday is a Jungle Friday\r\n").await?;

    loop {
        let line = match lines.next_line().await? {
            Some(s) => s,
            None => continue,
        };

        let mut split = line.split_ascii_whitespace();
        match split.next() {
            Some(cmd) if cmd.eq_ignore_ascii_case("QUIT") => {
                write_half.write_all(b"+OK Bye bye, kiss your homies goodnight no homo\r\n").await?;
                break;
            }
            Some(cmd) if cmd.eq_ignore_ascii_case("STAT") => {
                write_half.write_all(b"+OK 3 60\r\n").await?;
            }
            Some(cmd) if cmd.eq_ignore_ascii_case("LIST") => {
                if let Some(Ok(message_number)) = split.next().map(str::parse::<u32>) {
                    write_half
                        .write_all(match message_number {
                            1 => b"+OK 1 10\r\n",
                            2 => b"+OK 2 20\r\n",
                            3 => b"+OK 3 30\r\n",
                            _ => b"-ERR aight bro wtf that supposed to mean?\r\n",
                        })
                        .await?;
                } else {
                    write_half.write_all(b"+OK BOOMER\r\n1 10\r\n2 20\r\n3 30\r\n.\r\n").await?;
                }
            }
            Some(cmd) if cmd.eq_ignore_ascii_case("RETR") => {
                if let Some(Ok(message_number)) = split.next().map(str::parse::<u32>) {
                    write_half.write_all(match message_number {
                        1 => b"+OK Okie Dokie\r\n1111111111\r\n.\r\n",
                        2 => b"+OK Mambo Jambo\r\n222\r\n22222222\r\n22\r\n2\r\n.\r\n",
                        3 => b"+OK Dunga Bunga (see: https://en.wikipedia.org/wiki/Dunga_Bunga)\r\n33\r\n333333\r\n33333333\r\n33333\r\n3\r\n.\r\n",
                        _ => b"-ERR aight bro wtf that supposed to mean?\r\n",
                    }).await?;
                } else {
                    write_half.write_all(b"-ERR megamind_no_argument_questionmark.jpg\r\n").await?;
                }
            }
            Some(cmd) if cmd.eq_ignore_ascii_case("DELE") => {
                if let Some(Ok(_message_number)) = split.next().map(str::parse::<u32>) {
                    write_half.write_all(b"-ERR Imma say... no. (unimplemented)\r\n").await?;
                } else {
                    write_half.write_all(b"-ERR megamind_no_argument_questionmark.jpg\r\n").await?;
                }
            }
            Some(cmd) if cmd.eq_ignore_ascii_case("NOOP") => {
                write_half
                    .write_all(b"+OK Executing this command feels like I'm back at working at Globant\r\n")
                    .await?;
            }
            Some(cmd) if cmd.eq_ignore_ascii_case("RSET") => {
                write_half.write_all(b"+OK Yea sure why not\r\n").await?;
            }
            _ => {
                return Err(io::Error::new(
                    ErrorKind::Other,
                    "This was NOT how things were supposed to go :anger:",
                ))
            }
        }
    }

    println!("Bro is gon 💀");
    socket.shutdown().await?;
    Ok(())
}
