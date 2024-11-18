//! Contains utility functions related to sockets.

use std::{fmt, future::poll_fn, io::Error, net::SocketAddr, task::Poll};

use tokio::net::{TcpListener, TcpStream};

/// Provides the [`accept_from_any`] function.
pub trait AcceptFromAny {
    /// Accepts an incoming connection from any of the given [`TcpListeners`](TcpListener).
    ///
    /// Returns [`Ok`] with the new [`TcpStream`] and remote address on success, or [`Err`] if an error occurs in a
    /// [`TcpListener`], with the index of the listener and the error.
    async fn accept_from_any(&self) -> Result<(TcpStream, SocketAddr), (usize, Error)>;
}

impl AcceptFromAny for [TcpListener] {
    async fn accept_from_any(&self) -> Result<(TcpStream, SocketAddr), (usize, Error)> {
        poll_fn(|cx| {
            for (i, l) in self.iter().enumerate() {
                let poll_status = l.poll_accept(cx);
                if let Poll::Ready(result) = poll_status {
                    return Poll::Ready(result.map_err(|e| (i, e)));
                }
            }

            Poll::Pending
        })
        .await
    }
}

/// A wrapper struct around an [`Option<SocketAddr>`] which implements display to print the given [`SocketAddr`] when
/// [`Some`], or "unknwon" when [`None`].
pub struct PrintSockaddrOrUnknown(pub Option<SocketAddr>);

impl fmt::Display for PrintSockaddrOrUnknown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(addr) => addr.fmt(f),
            None => f.write_str("unknown"),
        }
    }
}
