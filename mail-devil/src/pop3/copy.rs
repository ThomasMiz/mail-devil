use std::io::{self, ErrorKind};

use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    select,
};

pub enum CopyError {
    ReaderError(io::Error),
    WriterError(io::Error),
}

/// Copies data from a buffered reader to a writer, transforming LF newlines to CRLF and byte-stuffing any lines that
/// begin with period '.' as required by the POP3 RFC.
///
/// Does not prepend an `+OK` status indicator nor a `CRLF.CRLF` at the end of the sequence.
///
/// If the reader does not end in a newline, a newline is appended at its end.
pub async fn copy<R, W>(buffer_size: usize, reader: &mut R, writer: &mut W) -> Result<(), CopyError>
where
    R: AsyncRead + Unpin + ?Sized,
    W: AsyncWrite + Unpin + ?Sized,
{
    let mut buf = vec![0u8; buffer_size].into_boxed_slice();
    let mut buf_start = 0;
    let mut buf_end = 0;

    let mut reader_ended = false;

    let mut last_char = 0;
    let mut insert_char = None;

    loop {
        let (buf_contents, buf_empty_space) = buf[buf_start..].split_at_mut(buf_end - buf_start);
        select! {
            biased;
            result = reader.read(buf_empty_space), if !buf_empty_space.is_empty() && !reader_ended => {
                let bytes_read_count = result.map_err(CopyError::ReaderError)?;
                reader_ended = bytes_read_count == 0;
                buf_end += bytes_read_count;

            }
            result = do_write(writer, buf_contents, &mut insert_char, &mut buf_start, &mut last_char) => {
                result?;
            }
        }

        if buf_start >= buf_end {
            if reader_ended {
                break;
            }

            buf_start = 0;
            buf_end = 0;
        }
    }

    if last_char != b'\n' {
        writer.write_all(b"\r\n").await.map_err(CopyError::WriterError)?;
    }

    Ok(())
}

async fn do_write<W>(
    writer: &mut W,
    buf_contents: &[u8],
    insert_char: &mut Option<u8>,
    buf_start: &mut usize,
    last_char: &mut u8,
) -> Result<(), CopyError>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    if let Some(c) = insert_char {
        let result = writer.write_u8(*c).await.map_err(CopyError::ReaderError);
        *last_char = *c;
        *insert_char = None;
        return result;
    }

    if buf_contents.is_empty() {
        return std::future::pending().await;
    }

    let (new_insert_char, i) = find_buffer_split_point(buf_contents, *last_char);

    let bytes_written_count = if i != 0 {
        let result = writer.write(&buf_contents[..i]).await;
        let written = result.map_err(CopyError::WriterError)?;
        if written == 0 {
            return Err(CopyError::WriterError(ErrorKind::UnexpectedEof.into()));
        }

        *buf_start += written;
        *last_char = buf_contents[written - 1];
        written
    } else {
        0
    };

    if bytes_written_count == i {
        *insert_char = new_insert_char;
    }

    Ok(())
}

fn find_buffer_split_point(buf_contents: &[u8], mut new_last_char: u8) -> (Option<u8>, usize) {
    for (i, b) in buf_contents.iter().copied().enumerate() {
        match b {
            b'\n' if new_last_char != b'\r' => return (Some(b'\r'), i),
            b'.' if new_last_char == b'\n' => return (Some(b'.'), i),
            _ => {}
        }

        new_last_char = b;
    }

    (None, buf_contents.len())
}
