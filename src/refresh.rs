use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

pub const OPTIONS: [u64; 7] = [1, 2, 5, 10, 30, 60, 300];

pub fn normalize(seconds: u64) -> u64 {
    if OPTIONS.contains(&seconds) {
        seconds
    } else {
        1
    }
}

pub struct RefreshRate {
    seconds: AtomicU64,
    lock: Mutex<()>,
    changed: Condvar,
}

impl RefreshRate {
    pub const fn new(seconds: u64) -> Self {
        Self {
            seconds: AtomicU64::new(seconds),
            lock: Mutex::new(()),
            changed: Condvar::new(),
        }
    }

    pub fn seconds(&self) -> u64 {
        self.seconds.load(Ordering::Relaxed)
    }

    pub fn set(&self, seconds: u64) {
        let _guard = self.lock.lock().unwrap();
        self.seconds.store(normalize(seconds), Ordering::Relaxed);
        self.changed.notify_all();
    }

    pub fn fresh_for(&self, minimum_seconds: u64) -> Duration {
        Duration::from_secs(self.seconds().max(minimum_seconds) + 5)
    }

    pub fn wait_after(&self, started: Instant, minimum_seconds: u64) {
        let mut guard = self.lock.lock().unwrap();
        loop {
            let interval = Duration::from_secs(self.seconds().max(minimum_seconds));
            let remaining = interval.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                break;
            }
            (guard, _) = self.changed.wait_timeout(guard, remaining).unwrap();
        }
    }
}

pub static CURRENT: RefreshRate = RefreshRate::new(1);

#[cfg(test)]
mod tests {
    use super::{RefreshRate, normalize};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    #[test]
    fn invalid_interval_falls_back_to_one_second() {
        assert_eq!(normalize(0), 1);
        assert_eq!(normalize(5), 5);
        assert_eq!(normalize(60), 60);
        assert_eq!(normalize(300), 300);
        assert_eq!(normalize(600), 1);
    }

    #[test]
    fn shortening_interval_wakes_a_waiting_collector() {
        let rate = Arc::new(RefreshRate::new(30));
        let worker_rate = Arc::clone(&rate);
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            worker_rate.wait_after(Instant::now(), 1);
            let _ = sender.send(());
        });
        std::thread::sleep(Duration::from_millis(100));
        rate.set(1);
        assert!(receiver.recv_timeout(Duration::from_secs(2)).is_ok());
    }
}
