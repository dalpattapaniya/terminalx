//! Permission waits belong to the authenticated owner, not a window lease.
//! Provider hooks still have finite timers: Claude 2.1.309 clamps command-hook
//! milliseconds to i32::MAX; Codex 0.153.4 accepts a positive u64 in seconds.
//! Stay below the smaller ceiling, with time for the bridge to return a denial.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

static LOCAL_OWNER: AtomicBool = AtomicBool::new(false);
static WAITERS: AtomicUsize = AtomicUsize::new(0);
const MAX_WAITERS: usize = 256;
pub(crate) const PROVIDER_SECONDS: u64 = i32::MAX as u64 / 1000;
pub(crate) const WAIT: Duration = Duration::from_secs(PROVIDER_SECONDS - 30);
pub(crate) const BRIDGE_WAIT: Duration = Duration::from_secs(PROVIDER_SECONDS - 15);

/// Called only by serve after acquiring the local data-directory owner lock.
/// This is not an environment setting or a client-supplied permission grant.
pub(crate) fn enable() { LOCAL_OWNER.store(true, Ordering::Release); }

pub(crate) fn enabled() -> bool { LOCAL_OWNER.load(Ordering::Acquire) }

pub(crate) struct Permit;
impl Permit {
    pub(crate) fn acquire() -> Option<Self> {
        WAITERS.fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| (n < MAX_WAITERS).then_some(n + 1)).ok().map(|_| Self)
    }
}
impl Drop for Permit {
    fn drop(&mut self) { WAITERS.fetch_sub(1, Ordering::AcqRel); }
}

/// Keep no runtime/store lock while waiting. Sender removal cancels immediately;
/// losing the hook connection cancels within one tick. Owning the receiver
/// ensures a late answer fails before the caller reacquires its runtime lock.
pub(crate) fn wait<T>(rx: Receiver<T>, limit: Duration, connected: impl Fn() -> bool) -> Option<T> {
    let started = Instant::now();
    loop {
        if !connected() { return None; }
        let remaining = limit.checked_sub(started.elapsed())?;
        match rx.recv_timeout(remaining.min(Duration::from_millis(100))) {
            Ok(answer) => return connected().then_some(answer),
            Err(RecvTimeoutError::Disconnected) => return None,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{mpsc, Arc};

    #[test]
    fn delayed_decision_and_cancellation_do_not_depend_on_a_view() {
        let (tx, rx) = mpsc::channel();
        let worker = std::thread::spawn(move || wait(rx, Duration::from_secs(2), || true));
        std::thread::sleep(Duration::from_millis(150));
        tx.send(7).unwrap();
        assert_eq!(worker.join().unwrap(), Some(7));
        let (tx, rx) = mpsc::channel::<bool>();
        drop(tx);
        assert_eq!(wait(rx, Duration::from_secs(2), || true), None);
    }

    #[test]
    fn disconnect_and_deadline_drop_the_receiver_before_a_stale_answer() {
        let connected = Arc::new(AtomicBool::new(true));
        let observed = connected.clone();
        let (tx, rx) = mpsc::channel::<bool>();
        let worker = std::thread::spawn(move || wait(rx, Duration::from_secs(2), || observed.load(Ordering::Acquire)));
        connected.store(false, Ordering::Release);
        assert_eq!(worker.join().unwrap(), None);
        assert!(tx.send(true).is_err());
        let (tx, rx) = mpsc::channel::<bool>();
        assert_eq!(wait(rx, Duration::from_millis(1), || true), None);
        assert!(tx.send(true).is_err());
    }

    #[test]
    fn waiter_budget_releases_on_all_exit_paths() {
        let permits = (0..MAX_WAITERS).map(|_| Permit::acquire().unwrap()).collect::<Vec<_>>();
        assert!(Permit::acquire().is_none());
        drop(permits);
        assert!(Permit::acquire().is_some());
        assert!(WAIT < BRIDGE_WAIT);
        assert!(BRIDGE_WAIT < Duration::from_secs(PROVIDER_SECONDS));
        assert!(PROVIDER_SECONDS * 1000 <= i32::MAX as u64);
    }
}
