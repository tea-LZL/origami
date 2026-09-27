//! Priority prefetch queue: user open preempts predicted/viewport/background
//! display fetches. Prefetch never touches flags; results after `cancel_all`
//! are discarded via a generation counter the fetch closure checks before
//! writing. Must be `run` from inside a Tokio runtime.

use std::collections::{HashSet, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::{Notify, Semaphore};

/// Dispatch priority; lower discriminant runs first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefetchPriority {
    Open = 0,
    Predictive = 1,
    Viewport = 2,
    Background = 3,
}

/// One unit of prefetch work, identified by a cache key.
#[derive(Debug, Clone)]
pub struct PrefetchRequest {
    pub key: String,
    pub priority: PrefetchPriority,
}

impl PrefetchRequest {
    pub fn new(key: impl Into<String>, priority: PrefetchPriority) -> Self {
        Self {
            key: key.into(),
            priority,
        }
    }
}

const MAX_CONSECUTIVE_FAILURES: usize = 5;

type BoxFuture = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;
type FetchFn = Arc<dyn Fn(String) -> BoxFuture + Send + Sync + 'static>;

struct Inner {
    buckets: [Mutex<VecDeque<PrefetchRequest>>; 4],
    active: Mutex<HashSet<String>>,
    generation: AtomicU64,
    paused: AtomicBool,
    enabled: AtomicBool,
    failures: AtomicUsize,
    semaphore: Arc<Semaphore>,
    notify: Notify,
}

/// Bounded-priority work queue with dedupe, cancellation, pause, and
/// failure-driven kill switch.
pub struct PrefetchQueue {
    inner: Arc<Inner>,
}

impl PrefetchQueue {
    pub fn new(max_concurrency: usize) -> Self {
        Self {
            inner: Arc::new(Inner {
                buckets: std::array::from_fn(|_| Mutex::new(VecDeque::new())),
                active: Mutex::new(HashSet::new()),
                generation: AtomicU64::new(0),
                paused: AtomicBool::new(false),
                enabled: AtomicBool::new(true),
                failures: AtomicUsize::new(0),
                semaphore: Arc::new(Semaphore::new(max_concurrency)),
                notify: Notify::new(),
            }),
        }
    }

    /// Queue `req`; `false` if disabled or the key is already queued/in flight.
    pub fn enqueue(&self, req: PrefetchRequest) -> bool {
        if !self.inner.enabled.load(Ordering::SeqCst) {
            return false;
        }
        {
            let mut active = self.inner.active.lock().unwrap();
            if !active.insert(req.key.clone()) {
                return false;
            }
        }
        self.inner.buckets[req.priority as usize]
            .lock()
            .unwrap()
            .push_back(req);
        self.inner.notify.notify_one();
        true
    }

    /// Drop all queued work; in-flight fetches finish but their results are
    /// stale (`generation()` changes) and must not be written.
    pub fn cancel_all(&self) {
        self.inner.generation.fetch_add(1, Ordering::SeqCst);
        let mut dropped = Vec::new();
        for bucket in &self.inner.buckets {
            let mut queue = bucket.lock().unwrap();
            while let Some(req) = queue.pop_front() {
                dropped.push(req.key);
            }
        }
        let mut active = self.inner.active.lock().unwrap();
        for key in dropped {
            active.remove(&key);
        }
    }

    /// Generation counter; fetch closures capture it and compare after
    /// `.await` to detect cancellation.
    pub fn generation(&self) -> u64 {
        self.inner.generation.load(Ordering::SeqCst)
    }

    pub fn set_paused(&self, paused: bool) {
        self.inner.paused.store(paused, Ordering::SeqCst);
        if !paused {
            self.inner.notify.notify_one();
        }
    }

    /// Kill switch (offline, account error, throttle). `true` resets the
    /// failure streak and re-enables.
    pub fn set_enabled(&self, enabled: bool) {
        self.inner.enabled.store(enabled, Ordering::SeqCst);
        if enabled {
            self.inner.failures.store(0, Ordering::SeqCst);
        }
        self.inner.notify.notify_one();
    }

    /// Record a fetch failure; `false` means the streak disabled prefetch.
    pub fn note_failure(&self) -> bool {
        record_failure(&self.inner)
    }

    pub fn note_success(&self) {
        record_success(&self.inner);
    }

    /// Spawn the dispatcher. `fetch` runs per key; its `Ok`/`Err` outcome
    /// resets or increments the failure streak automatically.
    pub fn run<F, Fut>(&self, fetch: F)
    where
        F: Fn(String) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        let fetch: FetchFn = Arc::new(move |key| Box::pin(fetch(key)));
        let inner = self.inner.clone();
        tokio::spawn(async move {
            loop {
                if inner.paused.load(Ordering::SeqCst) || !inner.enabled.load(Ordering::SeqCst) {
                    inner.notify.notified().await;
                    continue;
                }
                let Some(req) = pop_next(&inner) else {
                    inner.notify.notified().await;
                    continue;
                };
                let gen_at_pop = inner.generation.load(Ordering::SeqCst);
                let Ok(permit) = inner.semaphore.clone().acquire_owned().await else {
                    return;
                };
                // The request may have been cancelled while waiting for a
                // permit; never start a fetch that predates a cancel_all.
                if inner.generation.load(Ordering::SeqCst) != gen_at_pop {
                    inner.active.lock().unwrap().remove(&req.key);
                    continue;
                }
                if !inner.enabled.load(Ordering::SeqCst) {
                    inner.active.lock().unwrap().remove(&req.key);
                    continue;
                }
                if inner.paused.load(Ordering::SeqCst) {
                    inner.buckets[req.priority as usize]
                        .lock()
                        .unwrap()
                        .push_front(req);
                    continue;
                }
                let inner = inner.clone();
                let fetch = fetch.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    let _active = ActiveKeyGuard {
                        inner: inner.clone(),
                        key: req.key.clone(),
                    };
                    match fetch(req.key).await {
                        Ok(()) => record_success(&inner),
                        Err(_) => {
                            record_failure(&inner);
                        }
                    }
                });
            }
        });
    }
}

fn record_failure(inner: &Inner) -> bool {
    let failures = inner.failures.fetch_add(1, Ordering::SeqCst) + 1;
    if failures >= MAX_CONSECUTIVE_FAILURES {
        inner.enabled.store(false, Ordering::SeqCst);
        return false;
    }
    true
}

fn record_success(inner: &Inner) {
    inner.failures.store(0, Ordering::SeqCst);
}

/// Releases a key from the dedupe set even if the fetch future panics.
struct ActiveKeyGuard {
    inner: Arc<Inner>,
    key: String,
}

impl Drop for ActiveKeyGuard {
    fn drop(&mut self) {
        self.inner.active.lock().unwrap().remove(&self.key);
    }
}

fn pop_next(inner: &Inner) -> Option<PrefetchRequest> {
    for bucket in &inner.buckets {
        let mut queue = bucket.lock().unwrap();
        if let Some(req) = queue.pop_front() {
            return Some(req);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    #[tokio::test]
    async fn open_overtakes_prefetch() {
        static ORDER: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let queue = PrefetchQueue::new(1);
        assert!(queue.enqueue(PrefetchRequest::new("v1", PrefetchPriority::Viewport)));
        assert!(queue.enqueue(PrefetchRequest::new("v2", PrefetchPriority::Viewport)));
        assert!(queue.enqueue(PrefetchRequest::new("v3", PrefetchPriority::Viewport)));
        assert!(queue.enqueue(PrefetchRequest::new("o1", PrefetchPriority::Open)));

        queue.run(|k: String| {
            Box::pin(async move {
                ORDER.lock().unwrap().push(k);
                Ok(())
            })
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let order = ORDER.lock().unwrap().clone();
        assert_eq!(order.first().map(String::as_str), Some("o1"), "open must run first: {order:?}");
        assert_eq!(order.len(), 4, "all four fetched: {order:?}");
    }

    #[tokio::test]
    async fn queue_dedupes_same_message() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let queue = PrefetchQueue::new(1);
        assert!(queue.enqueue(PrefetchRequest::new("k", PrefetchPriority::Viewport)));
        assert!(!queue.enqueue(PrefetchRequest::new("k", PrefetchPriority::Viewport)));

        queue.run(|_k: String| {
            Box::pin(async move {
                CALLS.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(CALLS.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cancel_stops_writes() {
        let wrote = std::sync::Arc::new(AtomicUsize::new(0));
        let queue = std::sync::Arc::new(PrefetchQueue::new(1));
        assert!(queue.enqueue(PrefetchRequest::new("k", PrefetchPriority::Viewport)));

        let queue_handle = queue.clone();
        {
            let wrote = wrote.clone();
            let queue = queue.clone();
            queue_handle.run(move |k: String| {
                let wrote = wrote.clone();
                let queue = queue.clone();
                Box::pin(async move {
                    let generation = queue.generation();
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    if queue.generation() == generation {
                        wrote.fetch_add(1, Ordering::SeqCst);
                    }
                    drop(k);
                    Ok(())
                })
            });
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        queue.cancel_all();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        assert_eq!(wrote.load(Ordering::SeqCst), 0, "results after cancel_all must not be written");
    }

    #[tokio::test]
    async fn paused_defers_work() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let queue = PrefetchQueue::new(1);
        queue.set_paused(true);
        assert!(queue.enqueue(PrefetchRequest::new("k", PrefetchPriority::Viewport)));
        queue.run(|_k: String| {
            Box::pin(async move {
                CALLS.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(CALLS.load(Ordering::SeqCst), 0, "paused queue must not fetch");

        queue.set_paused(false);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(CALLS.load(Ordering::SeqCst), 1, "unpause must drain the queue");
    }

    #[tokio::test]
    async fn disabled_ignores_enqueue() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let queue = PrefetchQueue::new(1);
        queue.set_enabled(false);
        assert!(!queue.enqueue(PrefetchRequest::new("k", PrefetchPriority::Viewport)));

        queue.run(|_k: String| {
            Box::pin(async move {
                CALLS.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(CALLS.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn throttle_backoff_disables_prefetch() {
        let queue = PrefetchQueue::new(1);
        assert!(queue.note_failure());
        assert!(queue.note_failure());
        assert!(queue.note_failure());
        assert!(queue.note_failure());
        assert!(!queue.note_failure(), "fifth consecutive failure disables prefetch");

        assert!(!queue.enqueue(PrefetchRequest::new("k", PrefetchPriority::Viewport)));
        queue.set_enabled(true);
        assert!(queue.enqueue(PrefetchRequest::new("k", PrefetchPriority::Viewport)));
    }

    #[tokio::test]
    async fn cancel_releases_dedupe_keys() {
        let queue = PrefetchQueue::new(1);
        assert!(queue.enqueue(PrefetchRequest::new("k", PrefetchPriority::Viewport)));
        queue.cancel_all();
        assert!(
            queue.enqueue(PrefetchRequest::new("k", PrefetchPriority::Viewport)),
            "cancelled keys must be re-enqueueable"
        );
    }

    #[tokio::test]
    async fn cancel_before_start_never_fetches() {
        static STARTED: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let queue = PrefetchQueue::new(1);
        assert!(queue.enqueue(PrefetchRequest::new("slow", PrefetchPriority::Viewport)));
        assert!(queue.enqueue(PrefetchRequest::new("queued", PrefetchPriority::Viewport)));

        queue.run(|k: String| {
            Box::pin(async move {
                STARTED.lock().unwrap().push(k.clone());
                if k == "slow" {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
                Ok(())
            })
        });
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        queue.cancel_all();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let started = STARTED.lock().unwrap().clone();
        assert_eq!(started, vec!["slow".to_string()], "popped-but-waiting request must not start after cancel: {started:?}");
    }

    #[tokio::test]
    async fn panicking_fetch_releases_dedupe_key() {
        let queue = PrefetchQueue::new(1);
        assert!(queue.enqueue(PrefetchRequest::new("boom", PrefetchPriority::Viewport)));
        queue.run(|_k: String| {
            Box::pin(async move {
                panic!("fetch exploded");
            })
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            queue.enqueue(PrefetchRequest::new("boom", PrefetchPriority::Viewport)),
            "key must be released even when the fetch panics"
        );
    }
}
