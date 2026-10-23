use crate::Sample;

#[derive(Clone)]
pub struct Reading {
    pub key: String,
    pub group: String,
    pub name: String,
    pub value: Option<f64>,
    pub unit: &'static str,
    pub decimals: usize,
}

impl Reading {
    pub fn formatted(&self) -> String {
        match self.value {
            Some(value) if self.unit.is_empty() => format!("{:.*}", self.decimals, value),
            Some(value) => format!("{:.*} {}", self.decimals, value, self.unit),
            None => "Unavailable".to_owned(),
        }
    }

    pub fn format_value(&self, value: f64) -> String {
        if self.unit.is_empty() {
            format!("{:.*}", self.decimals, value)
        } else {
            format!("{:.*} {}", self.decimals, value, self.unit)
        }
    }
}

fn add(
    readings: &mut Vec<Reading>,
    key: String,
    group: String,
    name: &str,
    value: Option<f64>,
    unit: &'static str,
    decimals: usize,
) {
    readings.push(Reading {
        key,
        group,
        name: name.to_owned(),
        value,
        unit,
        decimals,
    });
}

pub fn collect_readings(sample: &Sample) -> Vec<Reading> {
    let mut readings = Vec::with_capacity(32 + sample.cores.len() + sample.sensors.len());
    let cpu = "Processor".to_owned();
    add(
        &mut readings,
        "cpu/usage".to_owned(),
        cpu.clone(),
        "Total usage",
        Some(sample.cpu_usage as f64),
        "%",
        1,
    );
    add(
        &mut readings,
        "cpu/clock".to_owned(),
        cpu.clone(),
        "Clock",
        (sample.cpu_mhz > 0).then_some(sample.cpu_mhz as f64),
        "MHz",
        0,
    );
    add(
        &mut readings,
        "cpu/temperature".to_owned(),
        cpu.clone(),
        "Package temperature",
        sample.cpu_temp.map(f64::from),
        "°C",
        1,
    );
    add(
        &mut readings,
        "cpu/power".to_owned(),
        cpu.clone(),
        "Package power",
        sample.cpu_power.map(f64::from),
        "W",
        1,
    );
    for (index, usage) in sample.cores.iter().enumerate() {
        add(
            &mut readings,
            format!("cpu/core/{index}"),
            cpu.clone(),
            &format!("Logical processor {index}"),
            Some(*usage as f64),
            "%",
            1,
        );
    }

    let memory = "Memory".to_owned();
    add(
        &mut readings,
        "ram/used".to_owned(),
        memory.clone(),
        "Used",
        Some(sample.ram_used as f64 / 1e9),
        "GB",
        2,
    );
    add(
        &mut readings,
        "ram/available".to_owned(),
        memory.clone(),
        "Available",
        Some(sample.ram_available as f64 / 1e9),
        "GB",
        2,
    );
    add(
        &mut readings,
        "ram/total".to_owned(),
        memory,
        "Installed",
        Some(sample.ram_total as f64 / 1e9),
        "GB",
        2,
    );

    for gpu in &sample.gpus {
        let group = format!("Graphics · {}", gpu.name);
        add(
            &mut readings,
            format!("gpu/{}/usage", gpu.id),
            group.clone(),
            "GPU usage",
            gpu.usage.map(f64::from),
            "%",
            1,
        );
        add(
            &mut readings,
            format!("gpu/{}/temperature", gpu.id),
            group.clone(),
            "GPU temperature",
            gpu.temp.map(f64::from),
            "°C",
            1,
        );
        add(
            &mut readings,
            format!("gpu/{}/power", gpu.id),
            group.clone(),
            "GPU power",
            gpu.power.map(f64::from),
            "W",
            1,
        );
        add(
            &mut readings,
            format!("gpu/{}/clock", gpu.id),
            group.clone(),
            "Graphics clock",
            gpu.clock_mhz.map(f64::from),
            "MHz",
            0,
        );
        add(
            &mut readings,
            format!("gpu/{}/vram", gpu.id),
            group,
            "VRAM used",
            gpu.vram_used.map(|value| value as f64 / 1e9),
            "GB",
            2,
        );
    }

    for drive in &sample.disks {
        let group = format!("Drive · {}", drive.mount);
        let key = drive.mount.to_ascii_lowercase();
        add(
            &mut readings,
            format!("drive/{key}/used"),
            group.clone(),
            "Space used",
            Some(drive.total.saturating_sub(drive.free) as f64 / 1e9),
            "GB",
            2,
        );
        add(
            &mut readings,
            format!("drive/{key}/free"),
            group.clone(),
            "Space free",
            Some(drive.free as f64 / 1e9),
            "GB",
            2,
        );
        add(
            &mut readings,
            format!("drive/{key}/total"),
            group,
            "Capacity",
            Some(drive.total as f64 / 1e9),
            "GB",
            2,
        );
        let group = format!("Drive · {}", drive.mount);
        add(
            &mut readings,
            format!("drive/{key}/read"),
            group.clone(),
            "Read rate",
            drive.read_bps.map(|value| value as f64 / 1e6),
            "MB/s",
            2,
        );
        add(
            &mut readings,
            format!("drive/{key}/write"),
            group.clone(),
            "Write rate",
            drive.write_bps.map(|value| value as f64 / 1e6),
            "MB/s",
            2,
        );
        add(
            &mut readings,
            format!("drive/{key}/activity"),
            group,
            "Activity",
            drive.active_pct.map(f64::from),
            "%",
            1,
        );
    }

    for sensor in &sample.sensors {
        let parent = sensor.parent.to_ascii_lowercase();
        let group = if parent.contains("cpu") {
            "Processor · additional sensors"
        } else if parent.contains("gpu") {
            "Graphics · additional sensors"
        } else if parent.contains("nvme") || parent.contains("hdd") {
            "Drives · additional sensors"
        } else {
            "System · additional sensors"
        };
        let unit = match sensor.sensor_type.as_str() {
            "Temperature" => "°C",
            "Power" => "W",
            "Fan" => "RPM",
            "Voltage" => "V",
            "Current" => "A",
            "Load" => "%",
            "Control" => "%",
            "Level" => "%",
            "Clock" => "MHz",
            "Frequency" => "Hz",
            "Data" => "GB",
            "SmallData" => "MB",
            "Throughput" => "B/s",
            "TimeSpan" => "s",
            "Timing" => "ns",
            "Energy" => "mWh",
            "Noise" => "dBA",
            "Conductivity" => "µS/cm",
            "Humidity" => "%",
            "Flow" => "L/h",
            _ => "",
        };
        let value = sensor.value.filter(|value| {
            !(sensor.sensor_type == "Power"
                && sensor.name.eq_ignore_ascii_case("CPU Package")
                && parent.contains("cpu")
                && *value <= 0.0)
        });
        add(
            &mut readings,
            format!(
                "lhm/{}/{}/{}",
                sensor.parent, sensor.sensor_type, sensor.name
            ),
            group.to_owned(),
            &sensor.name,
            value.map(f64::from),
            unit,
            1,
        );
    }
    readings
}

#[cfg(test)]
mod tests {
    use super::collect_readings;
    use crate::{Gpu, Sample, Sensor};

    #[test]
    fn zero_package_power_is_unavailable_in_sensor_table() {
        let sample = Sample {
            sensors: vec![Sensor {
                name: "CPU Package".to_owned(),
                sensor_type: "Power".to_owned(),
                value: Some(0.0),
                parent: "/intelcpu/0".to_owned(),
            }],
            ..Default::default()
        };
        let reading = collect_readings(&sample)
            .into_iter()
            .find(|reading| reading.key == "lhm//intelcpu/0/Power/CPU Package")
            .unwrap();
        assert_eq!(reading.formatted(), "Unavailable");
    }

    #[test]
    fn library_data_and_small_data_keep_their_native_units() {
        let sample = Sample {
            sensors: vec![
                Sensor {
                    name: "Memory Used".to_owned(),
                    sensor_type: "Data".to_owned(),
                    value: Some(2.5),
                    parent: "/ram".to_owned(),
                },
                Sensor {
                    name: "GPU Memory Used".to_owned(),
                    sensor_type: "SmallData".to_owned(),
                    value: Some(134.0),
                    parent: "/gpu-nvidia/0".to_owned(),
                },
            ],
            ..Default::default()
        };
        let readings = collect_readings(&sample);
        let data = readings
            .iter()
            .find(|reading| reading.key == "lhm//ram/Data/Memory Used")
            .unwrap();
        let small_data = readings
            .iter()
            .find(|reading| reading.key == "lhm//gpu-nvidia/0/SmallData/GPU Memory Used")
            .unwrap();
        assert_eq!(data.formatted(), "2.5 GB");
        assert_eq!(small_data.formatted(), "134.0 MB");
    }

    #[test]
    fn missing_hardware_sensor_is_unavailable_and_gpu_power_is_in_watts() {
        let sample = Sample {
            cpu_usage: 42.0,
            cpu_temp: None,
            gpus: vec![Gpu {
                id: "gpu-rtx".to_owned(),
                name: "RTX".to_owned(),
                power: Some(18.5),
                ..Default::default()
            }],
            ..Default::default()
        };
        let readings = collect_readings(&sample);

        let cpu_temp = readings
            .iter()
            .find(|reading| reading.key == "cpu/temperature")
            .unwrap();
        assert_eq!(cpu_temp.formatted(), "Unavailable");

        let gpu_power = readings
            .iter()
            .find(|reading| reading.key == "gpu/gpu-rtx/power")
            .unwrap();
        assert_eq!(gpu_power.formatted(), "18.5 W");
    }
}
