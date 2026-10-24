//! Per-provider rate limiting: a minimum interval between requests (requests/minute)
//! plus a cap on concurrent requests.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::Instant;

struct Entry {
    sem: Arc<Semaphore>,
    next: tokio::sync::Mutex<Instant>,
    interval: Duration,
}

#[derive(Default)]
pub struct RateLimits {
    map: Mutex<HashMap<String, Arc<Entry>>>,
}

pub struct Permit(#[allow(dead_code)] OwnedSemaphorePermit);

impl RateLimits {
    pub fn new() -> Self {
        Self::default()
    }

    /// Waits for a concurrency slot and for the rate window; the slot is held until the permit drops.
    pub async fn acquire(&self, id: &str, rpm: Option<i64>, max_concurrent: i64) -> Permit {
        let conc = max_concurrent.max(1) as usize;
        let key = format!("{id}:{}:{conc}", rpm.unwrap_or(0));
        let entry = {
            let mut m = self.map.lock().unwrap();
            m.entry(key)
                .or_insert_with(|| {
                    Arc::new(Entry {
                        sem: Arc::new(Semaphore::new(conc)),
                        next: tokio::sync::Mutex::new(Instant::now()),
                        interval: match rpm {
                            Some(r) if r > 0 => Duration::from_secs_f64(60.0 / r as f64),
                            _ => Duration::ZERO,
                        },
                    })
                })
                .clone()
        };
        let permit = entry.sem.clone().acquire_owned().await.expect("semaphore closed");
        if !entry.interval.is_zero() {
            let start = {
                let mut next = entry.next.lock().await;
                let start = (*next).max(Instant::now());
                *next = start + entry.interval;
                start
            };
            tokio::time::sleep_until(start).await;
        }
        Permit(permit)
    }
}
