use std::io;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

pub enum CopyError {
    ReaderError(io::Error),
    WriterError(io::Error),
}

/// Copies data from a buffered reader to a writer, transforming LF newlines to CRLF and byte-stuffing any lines that
/// begin with period '.' as required by the POP3 RFC.
///
/// Does not prepend an `+OK` status indicator nor a `CRLF.CRLF` at the end of the sequence.F
pub async fn copy<R, W>(reader: &mut R, writer: &mut W) -> Result<(), CopyError>
where
    R: AsyncBufRead + Unpin + ?Sized,
    W: AsyncWrite + Unpin + ?Sized,
{
    loop {
        let buf = reader.fill_buf().await.map_err(CopyError::ReaderError)?;
        if buf.is_empty() {
            break;
        }

        let mut last_char = 0;
        let mut last_i_written = 0;
        for i in 0..buf.len() {
            match buf[i] {
                b'\n' if last_char != b'\r' => {
                    let to_write = &buf[last_i_written..i];
                    writer.write_all(to_write).await.map_err(CopyError::WriterError)?;
                    writer.write_u8(b'\r').await.map_err(CopyError::WriterError)?;
                    last_i_written = i;
                }
                b'.' if last_char == b'\n' => {
                    let to_write = &buf[last_i_written..(i + 1)];
                    writer.write_all(to_write).await.map_err(CopyError::WriterError)?;
                    writer.write_u8(b'.').await.map_err(CopyError::WriterError)?;
                    last_i_written = i + 1;
                }
                _ => {}
            }

            last_char = buf[i];
        }

        writer.write_all(&buf[last_i_written..]).await.map_err(CopyError::WriterError)?;
        let bytes_read = buf.len();
        reader.consume(bytes_read);
    }

    Ok(())
}
