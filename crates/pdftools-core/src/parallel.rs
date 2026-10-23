//! Independent pieces of work (images, streams) spread over a few threads.
//!
//! Each piece in flight holds its own data (a decoded photo is tens of MB),
//! so the number of threads is capped: [`DEFAULT_THREADS`], or what the app
//! sets with [`set_max_threads`] on devices with little memory. Where threads
//! aren't available (WebAssembly) everything runs on the calling thread.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

/// Pieces of work done at the same time unless [`set_max_threads`] says otherwise.
pub const DEFAULT_THREADS: usize = 4;

static MAX_THREADS: AtomicUsize = AtomicUsize::new(DEFAULT_THREADS);

/// Caps the threads every operation uses from now on (at least one).
pub fn set_max_threads(threads: usize) {
    MAX_THREADS.store(threads.max(1), Ordering::Relaxed);
}

pub fn max_threads() -> usize {
    MAX_THREADS.load(Ordering::Relaxed)
}

/// `work` applied to every item on up to [`max_threads`] threads, results in
/// the items' order. `done` is called on this thread with the number of items
/// finished so far, as they finish.
pub(crate) fn map<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync, done: impl FnMut(usize)) -> Vec<R> {
    map_on(max_threads(), items, work, done)
}

/// [`map`] on (up to) `threads` threads.
pub(crate) fn map_on<T: Sync, R: Send>(threads: usize, items: &[T], work: impl Fn(&T) -> R + Sync, mut done: impl FnMut(usize)) -> Vec<R> {
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let threads = threads.min(cores).min(items.len()).max(1);
    if threads == 1 {
        return items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let result = work(item);
                done(index + 1);
                result
            })
            .collect();
    }

    let next = AtomicUsize::new(0);
    let mut results: Vec<Option<R>> = items.iter().map(|_| None).collect();
    std::thread::scope(|scope| {
        let (sender, receiver) = mpsc::channel();
        for _ in 0..threads {
            let sender = sender.clone();
            let (next, work) = (&next, &work);
            scope.spawn(move || loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(item) = items.get(index) else { break };
                if sender.send((index, work(item))).is_err() {
                    break;
                }
            });
        }
        drop(sender);
        // Ends early only if a worker panicked; the scope then re-raises it.
        for (finished, (index, result)) in receiver.iter().enumerate() {
            results[index] = Some(result);
            done(finished + 1);
        }
    });
    results.into_iter().map(|result| result.expect("every item was processed")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_order_and_reports_progress() {
        let items: Vec<u32> = (0..50).collect();
        for threads in [0, 1, 3, 8] {
            let mut finished = Vec::new();
            let squares = map_on(threads, &items, |n| n * n, |count| finished.push(count));
            assert_eq!(squares, items.iter().map(|n| n * n).collect::<Vec<_>>());
            assert_eq!(finished, (1..=50).collect::<Vec<_>>());
        }
        assert!(map_on(4, &[] as &[u8], |_| 0, |_| panic!("nothing to report")).is_empty());
    }

    #[test]
    fn threads_can_be_capped() {
        set_max_threads(0);
        assert_eq!(max_threads(), 1);
        set_max_threads(DEFAULT_THREADS);
        assert_eq!(max_threads(), DEFAULT_THREADS);
    }
}
