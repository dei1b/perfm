use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;
use wmi::WMIConnection;

#[derive(Clone, Copy)]
pub struct CpuClock {
    mhz: Option<u64>,
    per_logical: [Option<u64>; 64],
    updated_at: Option<Instant>,
}

impl Default for CpuClock {
    fn default() -> Self {
        Self {
            mhz: None,
            per_logical: [None; 64],
            updated_at: None,
        }
    }
}

impl CpuClock {
    fn is_fresh(self) -> bool {
        self.updated_at
            .is_some_and(|updated| updated.elapsed() <= Duration::from_secs(3))
    }

    pub fn current_mhz(self) -> Option<u64> {
        self.is_fresh().then_some(self.mhz).flatten()
    }

    pub fn logical_mhz(self, index: usize) -> Option<u64> {
        self.is_fresh()
            .then(|| self.per_logical.get(index).copied().flatten())
            .flatten()
    }

    fn from_rows(rows: Vec<ProcessorClockCounter>) -> Self {
        let mut clock = Self::default();
        for row in rows {
            let mhz = row.mhz.filter(|value| *value > 0);
            if row.name == "_Total" {
                clock.mhz = mhz;
            } else if let Some((group, logical)) = row.name.split_once(',')
                && group == "0"
                && let Ok(index) = logical.parse::<usize>()
                && let Some(slot) = clock.per_logical.get_mut(index)
            {
                *slot = mhz;
            }
        }
        if clock.mhz.is_some() || clock.per_logical.iter().any(Option::is_some) {
            clock.updated_at = Some(Instant::now());
        }
        clock
    }
}

#[derive(Deserialize)]
struct ProcessorClockCounter {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "ProcessorFrequency")]
    mhz: Option<u64>,
}

pub fn spawn_cpu_clock() -> Arc<RwLock<CpuClock>> {
    let shared = Arc::new(RwLock::new(CpuClock::default()));
    let weak = Arc::downgrade(&shared);
    let _ = thread::Builder::new()
        .name("perfm-cpu-clock".to_owned())
        .spawn(move || {
            let mut connection = WMIConnection::new().ok();
            while let Some(shared) = weak.upgrade() {
                let started = Instant::now();
                if connection.is_none() {
                    connection = WMIConnection::new().ok();
                }
                let result = connection.as_ref().map(|wmi| {
                    wmi.raw_query::<ProcessorClockCounter>(
                        "SELECT Name, ProcessorFrequency FROM Win32_PerfFormattedData_Counters_ProcessorInformation",
                    )
                });
                let clock = match result {
                    Some(Ok(rows)) => CpuClock::from_rows(rows),
                    Some(Err(_)) => {
                        connection = None;
                        CpuClock::default()
                    }
                    None => CpuClock::default(),
                };
                if let Ok(mut state) = shared.write() {
                    *state = clock;
                }
                thread::sleep(Duration::from_secs(1).saturating_sub(started.elapsed()));
            }
        });
    shared
}

#[cfg(test)]
mod tests {
    use super::{CpuClock, ProcessorClockCounter};

    #[test]
    fn maps_windows_frequency_to_each_logical_processor() {
        let clock = CpuClock::from_rows(vec![
            ProcessorClockCounter {
                name: "_Total".to_owned(),
                mhz: Some(2400),
            },
            ProcessorClockCounter {
                name: "0,0".to_owned(),
                mhz: Some(3100),
            },
            ProcessorClockCounter {
                name: "0,1".to_owned(),
                mhz: Some(1500),
            },
            ProcessorClockCounter {
                name: "1,0".to_owned(),
                mhz: Some(9000),
            },
        ]);
        assert_eq!(clock.current_mhz(), Some(2400));
        assert_eq!(clock.logical_mhz(0), Some(3100));
        assert_eq!(clock.logical_mhz(1), Some(1500));
        assert_eq!(clock.logical_mhz(64), None);
    }
}
