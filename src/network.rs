use std::time::{Duration, Instant};

use sysinfo::{InterfaceOperationalState, Networks};

#[derive(Clone, Default)]
pub struct AdapterReading {
    pub name: String,
    pub download_bps: Option<u64>,
    pub upload_bps: Option<u64>,
    pub connected: bool,
}

#[derive(Clone, Default)]
pub struct NetworkSample {
    pub adapters: Vec<AdapterReading>,
    pub download_bps: Option<u64>,
    pub upload_bps: Option<u64>,
    pub downloaded: u64,
    pub uploaded: u64,
}

pub struct NetworkSampler {
    networks: Networks,
    previous: Instant,
    downloaded: u64,
    uploaded: u64,
}

impl NetworkSampler {
    pub fn new() -> Self {
        Self {
            networks: Networks::new_with_refreshed_list(),
            previous: Instant::now(),
            downloaded: 0,
            uploaded: 0,
        }
    }

    pub fn sample(&mut self) -> NetworkSample {
        self.networks.refresh(true);
        let now = Instant::now();
        let elapsed = now.duration_since(self.previous);
        self.previous = now;
        let valid_rate = elapsed >= Duration::from_millis(250);
        let mut adapters = Vec::with_capacity(self.networks.len());
        let mut received = 0u64;
        let mut transmitted = 0u64;
        for (name, data) in &self.networks {
            let down = data.received();
            let up = data.transmitted();
            received = received.saturating_add(down);
            transmitted = transmitted.saturating_add(up);
            adapters.push(AdapterReading {
                name: name.clone(),
                download_bps: valid_rate
                    .then(|| bytes_per_second(down, elapsed))
                    .flatten(),
                upload_bps: valid_rate.then(|| bytes_per_second(up, elapsed)).flatten(),
                connected: matches!(
                    data.operational_state(),
                    InterfaceOperationalState::Up
                        | InterfaceOperationalState::Dormant
                        | InterfaceOperationalState::Unknown
                ),
            });
        }
        adapters.sort_by(|a, b| {
            b.connected
                .cmp(&a.connected)
                .then_with(|| {
                    let a_rate = a
                        .download_bps
                        .unwrap_or(0)
                        .saturating_add(a.upload_bps.unwrap_or(0));
                    let b_rate = b
                        .download_bps
                        .unwrap_or(0)
                        .saturating_add(b.upload_bps.unwrap_or(0));
                    b_rate.cmp(&a_rate)
                })
                .then_with(|| a.name.cmp(&b.name))
        });
        self.downloaded = self.downloaded.saturating_add(received);
        self.uploaded = self.uploaded.saturating_add(transmitted);
        NetworkSample {
            download_bps: (valid_rate && !adapters.is_empty())
                .then(|| bytes_per_second(received, elapsed))
                .flatten(),
            upload_bps: (valid_rate && !adapters.is_empty())
                .then(|| bytes_per_second(transmitted, elapsed))
                .flatten(),
            downloaded: self.downloaded,
            uploaded: self.uploaded,
            adapters,
        }
    }
}

fn bytes_per_second(bytes: u64, elapsed: Duration) -> Option<u64> {
    (!elapsed.is_zero()).then(|| (bytes as f64 / elapsed.as_secs_f64()).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::bytes_per_second;
    use std::time::Duration;

    #[test]
    fn rate_uses_actual_time_between_samples() {
        assert_eq!(
            bytes_per_second(3_000, Duration::from_millis(500)),
            Some(6_000)
        );
        assert_eq!(bytes_per_second(3_000, Duration::from_secs(2)), Some(1_500));
        assert_eq!(bytes_per_second(3_000, Duration::ZERO), None);
    }
}
