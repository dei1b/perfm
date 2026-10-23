use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;
use wmi::WMIConnection;

#[derive(Clone, Copy, Default)]
pub struct PerfRate {
    pub read_bps: Option<u64>,
    pub write_bps: Option<u64>,
    pub active_pct: Option<f32>,
    updated_at: Option<Instant>,
}

impl PerfRate {
    pub fn is_fresh(&self) -> bool {
        self.updated_at
            .is_some_and(|time| time.elapsed() <= Duration::from_secs(10))
    }
}

#[derive(Deserialize)]
struct LogicalDiskCounter {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "DiskReadBytesPerSec")]
    read_bps: Option<u64>,
    #[serde(rename = "DiskWriteBytesPerSec")]
    write_bps: Option<u64>,
    #[serde(rename = "PercentDiskTime")]
    active_pct: Option<u64>,
}

pub fn mount_key(mount: &str) -> String {
    mount.trim_end_matches(['\\', '/']).to_ascii_uppercase()
}

pub fn spawn_disk_perf() -> Arc<RwLock<HashMap<String, PerfRate>>> {
    let shared = Arc::new(RwLock::new(HashMap::new()));
    let weak = Arc::downgrade(&shared);
    let _ = thread::Builder::new().name("perfm-disk-io".to_owned()).spawn(move || {
        let mut connection = WMIConnection::new().ok();
        while let Some(shared) = weak.upgrade() {
            if connection.is_none() { connection = WMIConnection::new().ok(); }
            if let Some(wmi) = &connection {
                match wmi.raw_query::<LogicalDiskCounter>(
                    "SELECT Name, DiskReadBytesPerSec, DiskWriteBytesPerSec, PercentDiskTime FROM Win32_PerfFormattedData_PerfDisk_LogicalDisk"
                ) {
                    Ok(rows) => {
                        let values = rows.into_iter().filter(|row| row.name != "_Total")
                            .map(|row| (row.name.to_ascii_uppercase(), PerfRate {
                                read_bps: row.read_bps,
                                write_bps: row.write_bps,
                                active_pct: row.active_pct.map(|value| value as f32),
                                updated_at: Some(Instant::now()),
                            })).collect();
                        if let Ok(mut guard) = shared.write() { *guard = values; }
                    }
                    Err(_) => {
                        connection = None;
                        if let Ok(mut guard) = shared.write() { guard.clear(); }
                    }
                }
            }
            drop(shared);
            thread::sleep(Duration::from_secs(3));
        }
    });
    shared
}

#[cfg(test)]
mod tests {
    use super::mount_key;

    #[test]
    fn windows_mount_point_matches_logical_disk_counter_name() {
        assert_eq!(mount_key("C:\\"), "C:");
        assert_eq!(mount_key("d:\\"), "D:");
    }
}
