//! [`Paginator`]: every item of a cursor-paginated list.

use std::collections::{HashSet, VecDeque};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;

use crate::error::Result;
use crate::generated::types::CursorPage;

type PageFuture<T> = Pin<Box<dyn Future<Output = Result<CursorPage<T>>> + Send>>;
type FetchPage<T> = Box<dyn FnMut(Option<String>) -> PageFuture<T> + Send>;

/// Every item of a cursor-paginated list, one page request at a time.
///
/// Returned by the `iterate` methods, such as
/// [`Domains::iterate`](crate::resources::Domains::iterate). It requests the first page when
/// you ask for the first item, follows `next_cursor` until it is `None`, and stops when the API
/// repeats a cursor. After an error it ends.
///
/// Read it with the inherent [`next`](Self::next) method, or as a [`Stream`] (for example with
/// `futures::StreamExt` or `tokio_stream::StreamExt`). Dropping it stops the iteration and
/// cancels a page request in flight.
///
/// ```no_run
/// # async fn run(lettermint: lettermint::Lettermint) -> lettermint::Result<()> {
/// let mut domains = lettermint.domains().iterate(&Default::default());
/// while let Some(domain) = domains.next().await {
///     println!("{}", domain?.domain);
/// }
/// # Ok(())
/// # }
/// ```
pub struct Paginator<T> {
    fetch: FetchPage<T>,
    pending: Option<PageFuture<T>>,
    buffer: VecDeque<T>,
    cursor: Option<String>,
    seen: HashSet<String>,
    done: bool,
}

// The paginator never pins `T` or the page future in place: the future is boxed.
impl<T> Unpin for Paginator<T> {}

impl<T> Paginator<T> {
    pub(crate) fn new(fetch: impl FnMut(Option<String>) -> PageFuture<T> + Send + 'static) -> Self {
        Self {
            fetch: Box::new(fetch),
            pending: None,
            buffer: VecDeque::new(),
            cursor: None,
            seen: HashSet::new(),
            done: false,
        }
    }

    /// The next item, `None` after the last one.
    pub async fn next(&mut self) -> Option<Result<T>> {
        std::future::poll_fn(|context| Pin::new(&mut *self).poll_next(context)).await
    }
}

impl<T> Stream for Paginator<T> {
    type Item = Result<T>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            if let Some(item) = this.buffer.pop_front() {
                return Poll::Ready(Some(Ok(item)));
            }
            if this.done {
                return Poll::Ready(None);
            }
            if this.pending.is_none() {
                let cursor = this.cursor.take();
                this.pending = Some((this.fetch)(cursor));
            }
            let pending = this.pending.as_mut().expect("a page request is pending");
            match pending.as_mut().poll(context) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => {
                    this.pending = None;
                    this.done = true;
                    return Poll::Ready(Some(Err(error)));
                }
                Poll::Ready(Ok(page)) => {
                    this.pending = None;
                    this.buffer.extend(page.data);
                    match page.next_cursor {
                        Some(next) if !next.is_empty() && this.seen.insert(next.clone()) => {
                            this.cursor = Some(next)
                        }
                        _ => this.done = true,
                    }
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (
            self.buffer.len(),
            if self.done {
                Some(self.buffer.len())
            } else {
                None
            },
        )
    }
}

impl<T> fmt::Debug for Paginator<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Paginator")
            .field("buffered", &self.buffer.len())
            .field("done", &self.done)
            .finish_non_exhaustive()
    }
}
