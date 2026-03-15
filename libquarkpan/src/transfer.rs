use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;
use pin_project_lite::pin_project;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::QuarkPanError;

/// The latest known transfer progress.
#[derive(Debug, Clone, Copy, Default)]
pub struct TransferProgress {
    pub transferred: u64,
    pub total: Option<u64>,
}

#[derive(Clone)]
struct TransferInner {
    progress_tx: watch::Sender<TransferProgress>,
    cancel: CancellationToken,
}

/// Shared transfer control object used to observe progress and cancel work.
#[derive(Clone)]
pub struct TransferControl {
    inner: Arc<TransferInner>,
}

impl TransferControl {
    /// Creates a new transfer controller with an optional total byte count.
    pub fn new(total: Option<u64>) -> Self {
        let (progress_tx, _progress_rx) = watch::channel(TransferProgress {
            transferred: 0,
            total,
        });
        Self {
            inner: Arc::new(TransferInner {
                progress_tx,
                cancel: CancellationToken::new(),
            }),
        }
    }

    /// Returns the latest known progress snapshot.
    pub fn snapshot(&self) -> TransferProgress {
        *self.inner.progress_tx.borrow()
    }

    /// Subscribes to progress updates.
    pub fn subscribe(&self) -> watch::Receiver<TransferProgress> {
        self.inner.progress_tx.subscribe()
    }

    /// Cancels the associated transfer.
    pub fn cancel(&self) {
        self.inner.cancel.cancel();
    }

    /// Returns true if cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancel.is_cancelled()
    }

    /// Returns a cloneable token for integration with external cancellation logic.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.inner.cancel.clone()
    }

    fn advance(&self, delta: u64) {
        let mut current = *self.inner.progress_tx.borrow();
        current.transferred = current.transferred.saturating_add(delta);
        let _ = self.inner.progress_tx.send(current);
    }

    /// Marks the transfer as complete so progress UIs can render a final 100% state.
    pub fn finish(&self) {
        let mut current = *self.inner.progress_tx.borrow();
        if let Some(total) = current.total {
            current.transferred = total;
        }
        let _ = self.inner.progress_tx.send(current);
    }
}

pin_project! {
    /// Wraps a byte stream and updates a [`TransferControl`] as bytes are consumed.
    pub struct ProgressStream<S> {
        #[pin]
        inner: S,
        control: TransferControl,
    }
}

impl<S> ProgressStream<S> {
    /// Creates a new progress-aware wrapper around a byte stream.
    pub fn new(inner: S, control: TransferControl) -> Self {
        Self { inner, control }
    }
}

impl<S, E> Stream for ProgressStream<S>
where
    S: Stream<Item = Result<Bytes, E>>,
    E: Into<QuarkPanError>,
{
    type Item = Result<Bytes, QuarkPanError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        if this.control.is_cancelled() {
            return Poll::Ready(Some(Err(QuarkPanError::Cancelled)));
        }
        match this.inner.poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                this.control.advance(bytes.len() as u64);
                Poll::Ready(Some(Ok(bytes)))
            }
            Poll::Ready(Some(Err(err))) => Poll::Ready(Some(Err(err.into()))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
