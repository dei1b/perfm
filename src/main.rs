#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cpu_clock;
mod cpu_topology;
mod disk_perf;
mod lhm;
mod network;
mod readings;
mod stats;
mod theme;

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::os::windows::process::CommandExt;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use cpu_clock::{CpuClock, spawn_cpu_clock};
use cpu_topology::sensor_names;
use disk_perf::{PerfRate, mount_key, spawn_disk_perf};
use eframe::egui::{self, Color32, RichText};
use network::{NetworkSample, NetworkSampler};
use nvml_wrapper::{Nvml, enum_wrappers::device::TemperatureSensor};
use readings::{Reading, collect_readings};
use serde::Deserialize;
use stats::StatsBook;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use sysinfo::{Disks, System};
use theme::active as colors;

#[derive(Clone, Default)]
struct Sample {
    sampled_at: Option<Instant>,
    cpu_name: String,
    cpu_usage: f32,
    cpu_mhz: u64,
    cpu_clock_from_sensor: bool,
    cores: Vec<f32>,
    core_details: Vec<CoreDetail>,
    cpu_temp: Option<f32>,
    cpu_power: Option<f32>,
    ram_used: u64,
    ram_available: u64,
    ram_total: u64,
    disks: Vec<Drive>,
    network: NetworkSample,
    gpus: Vec<Gpu>,
    sensors: Vec<Sensor>,
    sensor_online: bool,
}

#[derive(Clone)]
struct Drive {
    name: String,
    mount: String,
    total: u64,
    free: u64,
    read_bps: Option<u64>,
    write_bps: Option<u64>,
    active_pct: Option<f32>,
}

#[derive(Clone, Default)]
struct Gpu {
    id: String,
    name: String,
    usage: Option<f32>,
    vram_used: Option<u64>,
    vram_total: Option<u64>,
    temp: Option<f32>,
    power: Option<f32>,
    clock_mhz: Option<u32>,
}

#[derive(Clone, Deserialize)]
struct Sensor {
    name: String,
    sensor_type: String,
    value: Option<f32>,
    parent: String,
}

#[derive(Clone, Default)]
struct CoreDetail {
    physical_core: Option<String>,
    clock_mhz: Option<u64>,
    temperature: Option<f32>,
    package_temperature: bool,
}

struct SensorReader {
    output: Option<BufReader<ChildStdout>>,
    cached: Vec<Sensor>,
}

#[derive(Default)]
struct SensorState {
    sensors: Vec<Sensor>,
    online: bool,
    updated_at: Option<Instant>,
}

struct CpuSources {
    sensors: Arc<RwLock<SensorState>>,
    clock: Arc<RwLock<CpuClock>>,
    core_sensor_names: Vec<Option<String>>,
}

impl SensorReader {
    fn new(output: Option<ChildStdout>) -> Self {
        Self {
            output: output.map(BufReader::new),
            cached: Vec::new(),
        }
    }

    fn read(&mut self) -> (&[Sensor], bool) {
        if let Some(output) = &mut self.output {
            let mut line = String::new();
            match output.read_line(&mut line) {
                Ok(0) | Err(_) => {
                    self.output = None;
                    self.cached.clear();
                }
                Ok(_) => {
                    self.cached = lhm::parse_sensor_line(&line).unwrap_or_default();
                }
            }
        }
        let online = !self.cached.is_empty();
        (&self.cached, online)
    }
}

fn start_sensor_bridge() -> Option<(Child, ChildStdout)> {
    let dir = std::env::current_exe().ok()?.parent()?.join("SensorBridge");
    let executable = dir.join("Perfm.SensorBridge.exe");
    if !executable.is_file() {
        return None;
    }
    let mut child = Command::new(executable)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .spawn()
        .ok()?;
    let output = child.stdout.take()?;
    Some((child, output))
}

fn stop_bundled_helper(helper: &mut Option<Child>) {
    if let Some(mut child) = helper.take()
        && child.try_wait().ok().flatten().is_none()
    {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn preferred_sensor(sensors: &[Sensor], sensor_type: &str, names: &[&str]) -> Option<f32> {
    names.iter().find_map(|target| {
        sensors
            .iter()
            .find(|sensor| {
                sensor.sensor_type.eq_ignore_ascii_case(sensor_type)
                    && sensor.name.eq_ignore_ascii_case(target)
                    && sensor.parent.to_ascii_lowercase().contains("cpu")
            })
            .and_then(|sensor| sensor.value.filter(|value| *value > 0.0))
    })
}

fn cpu_core_clock(sensors: &[Sensor]) -> Option<u64> {
    let clocks: Vec<f32> = sensors
        .iter()
        .filter(|sensor| {
            sensor.sensor_type.eq_ignore_ascii_case("Clock")
                && sensor.parent.to_ascii_lowercase().contains("cpu")
                && (sensor.name.starts_with("P-Core")
                    || sensor.name.starts_with("E-Core")
                    || sensor.name.starts_with("Core #")
                    || sensor.name.starts_with("CPU Core #"))
                && !sensor.name.contains("(Effective)")
        })
        .filter_map(|sensor| {
            sensor
                .value
                .filter(|value| value.is_finite() && *value > 0.0)
        })
        .collect();
    (!clocks.is_empty()).then(|| (clocks.iter().sum::<f32>() / clocks.len() as f32).round() as u64)
}

fn physical_core_reading(sensors: &[Sensor], sensor_type: &str, name: &str) -> Option<f32> {
    preferred_sensor(sensors, sensor_type, &[name]).or_else(|| {
        name.strip_prefix("Core #").and_then(|number| {
            preferred_sensor(sensors, sensor_type, &[&format!("CPU Core #{number}")])
        })
    })
}

fn spawn_sensor_reader(output: Option<ChildStdout>) -> Arc<RwLock<SensorState>> {
    let shared = Arc::new(RwLock::new(SensorState::default()));
    let weak = Arc::downgrade(&shared);
    let _ = thread::Builder::new()
        .name("perfm-sensors".to_owned())
        .spawn(move || {
            let mut reader = SensorReader::new(output);
            while let Some(shared) = weak.upgrade() {
                let (sensors, online) = reader.read();
                if let Ok(mut state) = shared.write() {
                    state.sensors = sensors.to_vec();
                    state.online = online;
                    state.updated_at = Some(Instant::now());
                }
                drop(shared);
                thread::sleep(Duration::from_secs(2));
            }
        });
    shared
}

fn collect_sample(
    system: &mut System,
    disks: &mut Disks,
    nvml: Option<&Nvml>,
    cpu_sources: &CpuSources,
    refresh_disks: bool,
    disk_perf: &Arc<RwLock<HashMap<String, PerfRate>>>,
    network_sampler: &mut NetworkSampler,
) -> Sample {
    system.refresh_cpu_all();
    system.refresh_memory();
    if refresh_disks {
        disks.refresh(true);
    }

    let (sensors, sensor_online) = cpu_sources
        .sensors
        .read()
        .map(|state| {
            if state
                .updated_at
                .is_some_and(|time| time.elapsed() <= Duration::from_secs(10))
            {
                (state.sensors.clone(), state.online)
            } else {
                (Vec::new(), false)
            }
        })
        .unwrap_or_default();
    let cpu_temp = preferred_sensor(&sensors, "Temperature", &["CPU Package"]);
    let cpu_power = preferred_sensor(&sensors, "Power", &["CPU Package"]);
    let windows_clock = cpu_sources
        .clock
        .read()
        .map(|clock| *clock)
        .unwrap_or_default();
    let windows_clock_mhz = windows_clock.current_mhz();
    let sensor_clock = windows_clock_mhz.or_else(|| cpu_core_clock(&sensors));
    let core_details = (0..system.cpus().len())
        .map(|index| {
            let physical_core = cpu_sources.core_sensor_names.get(index).cloned().flatten();
            let core_temperature = physical_core
                .as_deref()
                .and_then(|name| physical_core_reading(&sensors, "Temperature", name));
            let core_clock = physical_core
                .as_deref()
                .and_then(|name| physical_core_reading(&sensors, "Clock", name));
            CoreDetail {
                physical_core,
                clock_mhz: windows_clock
                    .logical_mhz(index)
                    .or_else(|| core_clock.map(|value| value.round() as u64)),
                temperature: core_temperature.or(cpu_temp),
                package_temperature: core_temperature.is_none(),
            }
        })
        .collect();
    let disk_rates = disk_perf
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    let network = network_sampler.sample();

    let gpus = nvml
        .map(|nvml| {
            (0..nvml.device_count().unwrap_or(0))
                .filter_map(|index| {
                    let device = nvml.device_by_index(index).ok()?;
                    let memory = device.memory_info().ok();
                    Some(Gpu {
                        id: device.uuid().unwrap_or_else(|_| {
                            device
                                .pci_info()
                                .map(|pci| pci.bus_id)
                                .unwrap_or_else(|_| format!("nvml-index-{index}"))
                        }),
                        name: device
                            .name()
                            .unwrap_or_else(|_| format!("NVIDIA GPU {index}")),
                        usage: device
                            .utilization_rates()
                            .ok()
                            .map(|value| value.gpu as f32),
                        vram_used: memory.as_ref().map(|value| value.used),
                        vram_total: memory.as_ref().map(|value| value.total),
                        temp: device
                            .temperature(TemperatureSensor::Gpu)
                            .ok()
                            .map(|value| value as f32),
                        power: device.power_usage().ok().map(|value| value as f32 / 1000.0),
                        clock_mhz: device
                            .clock_info(nvml_wrapper::enum_wrappers::device::Clock::Graphics)
                            .ok(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Sample {
        sampled_at: Some(Instant::now()),
        cpu_name: system
            .cpus()
            .first()
            .map(|cpu| cpu.brand().to_owned())
            .unwrap_or_else(|| "CPU".to_owned()),
        cpu_usage: system.global_cpu_usage(),
        cpu_mhz: sensor_clock.unwrap_or(0),
        cpu_clock_from_sensor: windows_clock_mhz.is_none() && sensor_clock.is_some(),
        cores: system.cpus().iter().map(|cpu| cpu.cpu_usage()).collect(),
        core_details,
        cpu_temp,
        cpu_power,
        ram_used: system.used_memory(),
        ram_available: system.available_memory(),
        ram_total: system.total_memory(),
        disks: disks
            .list()
            .iter()
            .map(|disk| {
                let mount = disk.mount_point().display().to_string();
                let rate = disk_rates
                    .get(&mount_key(&mount))
                    .copied()
                    .filter(|rate| rate.is_fresh())
                    .unwrap_or_default();
                Drive {
                    name: disk.name().to_string_lossy().into_owned(),
                    mount,
                    total: disk.total_space(),
                    free: disk.available_space(),
                    read_bps: rate.read_bps,
                    write_bps: rate.write_bps,
                    active_pct: rate.active_pct,
                }
            })
            .collect(),
        network,
        gpus,
        sensors,
        sensor_online,
    }
}

fn spawn_sampler(output: Option<ChildStdout>) -> Receiver<Sample> {
    let (sender, receiver) = mpsc::sync_channel(2);
    thread::spawn(move || {
        let mut system = System::new();
        system.refresh_cpu_all();
        thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        let mut disks = Disks::new_with_refreshed_list();
        let nvml = Nvml::init().ok();
        let cpu_sources = CpuSources {
            sensors: spawn_sensor_reader(output),
            clock: spawn_cpu_clock(),
            core_sensor_names: sensor_names(system.cpus().len()),
        };
        let disk_perf = spawn_disk_perf();
        let mut network_sampler = NetworkSampler::new();
        let mut iteration = 0u64;
        loop {
            let started = Instant::now();
            let sample = collect_sample(
                &mut system,
                &mut disks,
                nvml.as_ref(),
                &cpu_sources,
                iteration.is_multiple_of(5),
                &disk_perf,
                &mut network_sampler,
            );
            iteration = iteration.wrapping_add(1);
            if sender.send(sample).is_err() {
                break;
            }
            thread::sleep(Duration::from_secs(1).saturating_sub(started.elapsed()));
        }
    });
    receiver
}

struct PerfmApp {
    helper: Option<Child>,
    receiver: Receiver<Sample>,
    sample: Sample,
    has_sample: bool,
    cpu_history: VecDeque<Option<f32>>,
    gpu_history: VecDeque<Option<f32>>,
    ram_history: VecDeque<Option<f32>>,
    download_history: VecDeque<Option<u64>>,
    upload_history: VecDeque<Option<u64>>,
    readings: Vec<Reading>,
    stats: StatsBook,
    tab: Tab,
    search: String,
    theme_index: usize,
    settings_error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Overview,
    Sensors,
    Settings,
}

impl PerfmApp {
    fn new() -> Self {
        let theme_index = theme::load();
        theme::select(theme_index);
        let (helper, output) = match start_sensor_bridge() {
            Some((helper, output)) => (Some(helper), Some(output)),
            None => (None, None),
        };
        Self {
            helper,
            receiver: spawn_sampler(output),
            sample: Sample::default(),
            has_sample: false,
            cpu_history: VecDeque::with_capacity(90),
            gpu_history: VecDeque::with_capacity(90),
            ram_history: VecDeque::with_capacity(90),
            download_history: VecDeque::with_capacity(90),
            upload_history: VecDeque::with_capacity(90),
            readings: Vec::new(),
            stats: StatsBook::default(),
            tab: Tab::Overview,
            search: String::new(),
            theme_index,
            settings_error: None,
        }
    }

    fn consume_samples(&mut self) {
        while let Ok(sample) = self.receiver.try_recv() {
            self.readings = collect_readings(&sample);
            for reading in &self.readings {
                if let Some(value) = reading.value {
                    self.stats.record(&reading.key, value);
                }
            }
            push_history(&mut self.cpu_history, Some(sample.cpu_usage));
            push_history(
                &mut self.gpu_history,
                sample.gpus.first().and_then(|gpu| gpu.usage),
            );
            push_history(
                &mut self.ram_history,
                Some(percent(sample.ram_used, sample.ram_total)),
            );
            push_rate_history(&mut self.download_history, sample.network.download_bps);
            push_rate_history(&mut self.upload_history, sample.network.upload_bps);
            self.sample = sample;
            self.has_sample = true;
        }
    }
}

impl Drop for PerfmApp {
    fn drop(&mut self) {
        stop_bundled_helper(&mut self.helper);
    }
}

impl eframe::App for PerfmApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.consume_samples();
        ctx.request_repaint_after(Duration::from_secs(1));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = colors().bg;
        visuals.window_fill = colors().bg;
        visuals.override_text_color = Some(colors().text);
        visuals.faint_bg_color = colors().panel;
        visuals.extreme_bg_color = colors().bg;
        visuals.code_bg_color = colors().panel;
        visuals.hyperlink_color = colors().nav;
        visuals.widgets.inactive.bg_fill = colors().panel;
        visuals.widgets.hovered.bg_fill = colors().border;
        visuals.widgets.active.bg_fill = colors().nav;
        visuals.selection.bg_fill = colors().nav.gamma_multiply(0.3);
        visuals.selection.stroke.color = colors().text;
        ui.ctx().set_visuals(visuals);
        egui::Frame::new().fill(colors().bg).show(ui, |ui| {
            let viewport = ui.available_size();
            ui.horizontal_top(|ui| {
                sidebar(ui, &mut self.tab);
                ui.allocate_ui_with_layout(
                    egui::vec2((viewport.x - 68.0).max(100.0), viewport.y),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        egui::Frame::new()
                            .fill(colors().bg)
                            .inner_margin(egui::Margin::symmetric(22, 20))
                            .show(ui, |ui| {
                                topbar(
                                    ui,
                                    &self.sample,
                                    self.has_sample,
                                    self.readings.len(),
                                    self.tab,
                                );
                                ui.add_space(22.0);
                                if !self.has_sample && self.tab != Tab::Settings {
                                    ui.label(
                                        RichText::new("Collecting the first reading…")
                                            .color(colors().muted),
                                    );
                                    return;
                                }
                                egui::ScrollArea::vertical()
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| match self.tab {
                                        Tab::Overview => {
                                            overview(
                                                ui,
                                                &self.sample,
                                                &self.cpu_history,
                                                &self.gpu_history,
                                                &self.ram_history,
                                                (&self.download_history, &self.upload_history),
                                                &mut self.tab,
                                            );
                                        }
                                        Tab::Sensors => {
                                            sensor_table(
                                                ui,
                                                &self.readings,
                                                &mut self.stats,
                                                &mut self.search,
                                            );
                                        }
                                        Tab::Settings => settings_view(
                                            ui,
                                            &mut self.theme_index,
                                            &mut self.settings_error,
                                        ),
                                    });
                            });
                    },
                );
            });
        });
    }
}

fn sidebar(ui: &mut egui::Ui, tab: &mut Tab) {
    egui::Frame::new()
        .fill(colors().rail)
        .stroke(egui::Stroke::new(1.0, colors().border))
        .inner_margin(egui::Margin::symmetric(8, 17))
        .show(ui, |ui| {
            ui.set_width(48.0);
            ui.set_min_height(ui.available_height());
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("P").size(24.0).strong().color(colors().text));
                ui.add_space(24.0);
                for (target, title) in [
                    (Tab::Overview, "Overview"),
                    (Tab::Sensors, "All sensors"),
                    (Tab::Settings, "Settings"),
                ] {
                    let active = *tab == target;
                    let response = ui
                        .add_sized(
                            [42.0, 42.0],
                            egui::Button::new("")
                                .fill(if active {
                                    colors().nav.gamma_multiply(0.25)
                                } else {
                                    colors().rail
                                })
                                .stroke(egui::Stroke::NONE)
                                .corner_radius(7),
                        )
                        .on_hover_text(title);
                    let center = response.rect.center();
                    let color = if active { colors().nav } else { colors().muted };
                    let painter = ui.painter();
                    match target {
                        Tab::Overview => {
                            let points = [(-12.0, 5.0), (-5.0, -2.0), (1.0, 2.0), (10.0, -8.0)];
                            for pair in points.windows(2) {
                                painter.line_segment(
                                    [
                                        center + egui::vec2(pair[0].0, pair[0].1),
                                        center + egui::vec2(pair[1].0, pair[1].1),
                                    ],
                                    egui::Stroke::new(1.8, color),
                                );
                            }
                        }
                        Tab::Sensors => {
                            for y in [-7.0, 0.0, 7.0] {
                                painter.line_segment(
                                    [center + egui::vec2(-10.0, y), center + egui::vec2(10.0, y)],
                                    egui::Stroke::new(1.8, color),
                                );
                            }
                        }
                        Tab::Settings => {
                            painter.circle_stroke(center, 8.0, egui::Stroke::new(1.7, color));
                            painter.circle_stroke(center, 2.6, egui::Stroke::new(1.7, color));
                            for angle in 0..8 {
                                let radians = angle as f32 * std::f32::consts::TAU / 8.0;
                                let direction = egui::vec2(radians.cos(), radians.sin());
                                painter.line_segment(
                                    [center + direction * 10.0, center + direction * 13.0],
                                    egui::Stroke::new(1.7, color),
                                );
                            }
                        }
                    }
                    if response.clicked() {
                        *tab = target;
                    }
                    ui.add_space(6.0);
                }
            });
        });
}

fn topbar(ui: &mut egui::Ui, sample: &Sample, has_sample: bool, count: usize, tab: Tab) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new(if tab == Tab::Settings {
                    "Settings"
                } else {
                    "Monitoring"
                })
                .size(25.0)
                .strong()
                .color(colors().text),
            );
            ui.label(
                RichText::new("Perfm  /  This device")
                    .size(11.0)
                    .color(colors().muted),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (status, color) = if !has_sample {
                ("Starting", colors().muted)
            } else if sample
                .sampled_at
                .is_some_and(|time| time.elapsed() <= Duration::from_secs(3))
            {
                ("Live", colors().success)
            } else {
                ("Stale", colors().warning)
            };
            egui::Frame::new()
                .fill(colors().panel)
                .stroke(egui::Stroke::new(1.0, colors().border))
                .corner_radius(6)
                .inner_margin(egui::Margin::symmetric(11, 7))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (mark, _) =
                            ui.allocate_exact_size(egui::vec2(8.0, 10.0), egui::Sense::hover());
                        ui.painter().circle_filled(mark.center(), 3.0, color);
                        ui.label(RichText::new(status).size(11.0).strong().color(color));
                    });
                });
            ui.add_space(9.0);
            ui.label(
                RichText::new(format!("{count} readings  ·  1 sec refresh"))
                    .size(11.0)
                    .color(colors().muted),
            );
        });
    });
}

fn settings_view(ui: &mut egui::Ui, selected: &mut usize, error: &mut Option<String>) {
    ui.label(
        RichText::new("Appearance")
            .size(18.0)
            .strong()
            .color(colors().text),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new("Choose a palette for the dashboard. Changes apply immediately and are saved on this device.")
            .size(11.0)
            .color(colors().muted),
    );
    ui.add_space(18.0);
    egui::Frame::new()
        .fill(colors().panel)
        .stroke(egui::Stroke::new(1.0, colors().border))
        .corner_radius(7)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_min_width((ui.available_width() - 32.0).max(160.0));
            ui.label(
                RichText::new("Current theme")
                    .size(10.5)
                    .color(colors().muted),
            );
            ui.label(
                RichText::new(theme::THEMES[*selected].name)
                    .size(20.0)
                    .strong()
                    .color(colors().text),
            );
            ui.label(
                RichText::new(theme::THEMES[*selected].description)
                    .size(11.0)
                    .color(colors().muted),
            );
        });
    if let Some(message) = error {
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!("Could not save the theme: {message}"))
                .size(11.0)
                .color(Color32::from_rgb(230, 106, 106)),
        );
    }
    ui.add_space(20.0);
    ui.label(
        RichText::new("Available themes")
            .size(13.0)
            .strong()
            .color(colors().text),
    );
    ui.add_space(10.0);
    let columns_count = if ui.available_width() >= 820.0 { 3 } else { 2 };
    let mut new_selection = None;
    ui.columns(columns_count, |columns| {
        for (index, palette) in theme::THEMES.iter().enumerate() {
            let column = &mut columns[index % columns_count];
            if theme_choice(column, palette, index == *selected) {
                new_selection = Some(index);
            }
            column.add_space(10.0);
        }
    });
    if let Some(index) = new_selection {
        *selected = index;
        theme::select(index);
        *error = theme::save(index).err().map(|error| error.to_string());
        ui.ctx().request_repaint();
    }
}

fn theme_choice(ui: &mut egui::Ui, palette: &theme::Theme, selected: bool) -> bool {
    let response = ui
        .add_sized(
            [ui.available_width(), 82.0],
            egui::Button::new(
                RichText::new(palette.name)
                    .size(12.0)
                    .strong()
                    .color(palette.text),
            )
            .fill(palette.panel)
            .stroke(egui::Stroke::new(
                if selected { 2.0 } else { 1.0 },
                if selected {
                    colors().nav
                } else {
                    palette.border
                },
            ))
            .corner_radius(7),
        )
        .on_hover_text(palette.description);
    let rect = response.rect;
    let swatches = [
        palette.bg,
        palette.panel,
        palette.series_a,
        palette.series_b,
        palette.storage,
    ];
    let swatch_width = 24.0;
    let gap = 5.0;
    let total = swatches.len() as f32 * swatch_width + (swatches.len() - 1) as f32 * gap;
    let start = rect.center().x - total / 2.0;
    for (index, swatch) in swatches.into_iter().enumerate() {
        ui.painter().rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(
                    start + index as f32 * (swatch_width + gap),
                    rect.bottom() - 23.0,
                ),
                egui::vec2(swatch_width, 11.0),
            ),
            2.0,
            swatch,
        );
    }
    if selected {
        let center = rect.right_top() + egui::vec2(-17.0, 17.0);
        ui.painter().circle_filled(center, 9.0, colors().nav);
        ui.painter().line_segment(
            [
                center + egui::vec2(-4.0, 0.0),
                center + egui::vec2(-1.0, 3.0),
            ],
            egui::Stroke::new(2.0, palette.bg),
        );
        ui.painter().line_segment(
            [
                center + egui::vec2(-1.0, 3.0),
                center + egui::vec2(5.0, -4.0),
            ],
            egui::Stroke::new(2.0, palette.bg),
        );
    }
    response.clicked()
}

fn overview(
    ui: &mut egui::Ui,
    sample: &Sample,
    cpu_history: &VecDeque<Option<f32>>,
    gpu_history: &VecDeque<Option<f32>>,
    ram_history: &VecDeque<Option<f32>>,
    network_history: (&VecDeque<Option<u64>>, &VecDeque<Option<u64>>),
    tab: &mut Tab,
) {
    if ui.available_width() >= 900.0 {
        ui.columns(4, |columns| {
            drives_card(&mut columns[0], sample);
            ram_card(&mut columns[1], sample, ram_history);
            gpu_card(&mut columns[2], sample, gpu_history);
            network_card(
                &mut columns[3],
                sample,
                network_history.0,
                network_history.1,
            );
        });
    } else {
        ui.columns(2, |columns| {
            drives_card(&mut columns[0], sample);
            ram_card(&mut columns[1], sample, ram_history);
        });
        ui.add_space(11.0);
        ui.columns(2, |columns| {
            gpu_card(&mut columns[0], sample, gpu_history);
            network_card(
                &mut columns[1],
                sample,
                network_history.0,
                network_history.1,
            );
        });
    }
    ui.add_space(22.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("System performance")
                .size(16.0)
                .strong()
                .color(colors().text),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new("Current readings")
                    .size(11.0)
                    .color(colors().muted),
            );
        });
    });
    ui.add_space(10.0);
    ui.columns(4, |columns| {
        stat_tile(
            &mut columns[0],
            "CPU usage",
            &format!("{:.0}%", sample.cpu_usage),
            "Across all logical processors",
            colors().cpu,
        );
        stat_tile(
            &mut columns[1],
            "CPU clock",
            &if sample.cpu_mhz > 0 {
                format!("{} MHz", sample.cpu_mhz)
            } else {
                "Unavailable".to_owned()
            },
            if sample.cpu_clock_from_sensor {
                "Sensor core average"
            } else {
                "Current CPU frequency"
            },
            colors().series_a,
        );
        stat_tile(
            &mut columns[2],
            "CPU temperature",
            &reading(sample.cpu_temp, " °C"),
            "Package sensor",
            colors().series_b,
        );
        stat_tile(
            &mut columns[3],
            "GPU power",
            &reading(sample.gpus.first().and_then(|gpu| gpu.power), " W"),
            "NVIDIA reported draw",
            colors().storage,
        );
    });
    ui.add_space(18.0);
    cpu_card(ui, sample, cpu_history);
    ui.add_space(14.0);
    sensor_card(ui, sample, tab);
    ui.add_space(18.0);
}

fn push_history(history: &mut VecDeque<Option<f32>>, value: Option<f32>) {
    if history.len() == 90 {
        history.pop_front();
    }
    history.push_back(value.map(|value| value.clamp(0.0, 100.0)));
}

fn push_rate_history(history: &mut VecDeque<Option<u64>>, value: Option<u64>) {
    if history.len() == 90 {
        history.pop_front();
    }
    history.push_back(value);
}

fn percent(used: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        used as f32 / total as f32 * 100.0
    }
}

fn bytes(value: u64) -> String {
    if value >= 1_000_000_000_000 {
        format!("{:.1} TB", value as f64 / 1e12)
    } else if value >= 1_000_000_000 {
        format!("{:.1} GB", value as f64 / 1e9)
    } else {
        format!("{:.1} MB", value as f64 / 1e6)
    }
}

fn rate(value: u64) -> String {
    if value >= 1_000_000_000 {
        format!("{:.1} GB/s", value as f64 / 1e9)
    } else if value >= 1_000_000 {
        format!("{:.1} MB/s", value as f64 / 1e6)
    } else if value >= 1_000 {
        format!("{:.1} KB/s", value as f64 / 1e3)
    } else {
        format!("{value} B/s")
    }
}

fn network_bytes(value: u64) -> String {
    if value >= 1_000_000_000 {
        format!("{:.1} GB", value as f64 / 1e9)
    } else if value >= 1_000_000 {
        format!("{:.1} MB", value as f64 / 1e6)
    } else if value >= 1_000 {
        format!("{:.1} KB", value as f64 / 1e3)
    } else {
        format!("{value} B")
    }
}

fn reading(value: Option<f32>, suffix: &str) -> String {
    value
        .map(|value| format!("{value:.1}{suffix}"))
        .unwrap_or_else(|| "Unavailable".to_owned())
}

fn card(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    accent: Color32,
    body: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::new()
        .fill(colors().panel)
        .stroke(egui::Stroke::new(1.0, colors().border))
        .corner_radius(egui::CornerRadius::same(7))
        .inner_margin(egui::Margin::same(15))
        .show(ui, |ui| {
            ui.set_min_width((ui.available_width() - 30.0).max(160.0));
            ui.set_min_height(318.0);
            ui.horizontal(|ui| {
                let (mark, _) =
                    ui.allocate_exact_size(egui::vec2(10.0, 12.0), egui::Sense::hover());
                ui.painter().circle_filled(mark.center(), 3.0, accent);
                ui.label(
                    RichText::new(title)
                        .size(13.0)
                        .strong()
                        .color(colors().text),
                );
            });
            ui.label(RichText::new(subtitle).size(10.5).color(colors().muted));
            ui.add_space(9.0);
            ui.separator();
            ui.add_space(7.0);
            body(ui);
        });
}

fn main_metric(ui: &mut egui::Ui, value: &str, caption: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(value)
                .size(29.0)
                .strong()
                .color(colors().text),
        );
        ui.add_space(5.0);
        ui.label(RichText::new(caption).size(10.5).color(colors().muted));
    });
}

fn stat_tile(ui: &mut egui::Ui, title: &str, value: &str, description: &str, accent: Color32) {
    egui::Frame::new()
        .fill(colors().panel)
        .stroke(egui::Stroke::new(1.0, colors().border))
        .corner_radius(6)
        .inner_margin(egui::Margin::same(13))
        .show(ui, |ui| {
            ui.set_min_width((ui.available_width() - 26.0).max(115.0));
            ui.set_min_height(92.0);
            ui.label(RichText::new(title).size(11.0).color(colors().muted));
            ui.add_space(7.0);
            ui.label(RichText::new(value).size(21.0).strong().color(accent));
            ui.label(RichText::new(description).size(10.0).color(colors().muted));
        });
}

fn metric_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(10.5).color(colors().muted));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(value)
                    .size(11.0)
                    .strong()
                    .color(colors().text),
            );
        });
    });
}

fn sparkline(ui: &mut egui::Ui, history: &VecDeque<Option<f32>>, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 103.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    for fraction in [0.25, 0.5, 0.75] {
        let y = rect.bottom() - fraction * rect.height();
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.0, colors().border.gamma_multiply(0.75)),
        );
    }
    let mut previous = None;
    for (index, value) in history.iter().enumerate() {
        let current = value.map(|value| {
            let x = rect.right() - (history.len() - 1 - index) as f32 / 89.0 * rect.width();
            let y = rect.bottom() - value / 100.0 * rect.height();
            egui::pos2(x, y)
        });
        if let (Some(start), Some(end)) = (previous, current) {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    start,
                    end,
                    egui::pos2(end.x, rect.bottom()),
                    egui::pos2(start.x, rect.bottom()),
                ],
                color.gamma_multiply(0.16),
                egui::Stroke::NONE,
            ));
            painter.line_segment([start, end], egui::Stroke::new(1.8, color));
        }
        previous = current;
    }
}

fn storage_ring(ui: &mut egui::Ui, used_percent: f32) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 154.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    let center = rect.center();
    let radius = 55.0;
    painter.circle_stroke(center, radius, egui::Stroke::new(17.0, colors().border));
    let segments = 100;
    let count = (used_percent.clamp(0.0, 100.0) / 100.0 * segments as f32).round() as usize;
    for index in 0..count {
        let start =
            -std::f32::consts::FRAC_PI_2 + index as f32 / segments as f32 * std::f32::consts::TAU;
        let end = -std::f32::consts::FRAC_PI_2
            + (index + 1) as f32 / segments as f32 * std::f32::consts::TAU;
        let point = |angle: f32| center + egui::vec2(radius * angle.cos(), radius * angle.sin());
        painter.line_segment(
            [point(start), point(end)],
            egui::Stroke::new(17.0, colors().storage),
        );
    }
    painter.text(
        center + egui::vec2(0.0, -12.0),
        egui::Align2::CENTER_CENTER,
        "Used",
        egui::FontId::proportional(10.0),
        colors().muted,
    );
    painter.text(
        center + egui::vec2(0.0, 12.0),
        egui::Align2::CENTER_CENTER,
        format!("{used_percent:.0}%"),
        egui::FontId::proportional(24.0),
        colors().text,
    );
}

fn cpu_card(ui: &mut egui::Ui, sample: &Sample, history: &VecDeque<Option<f32>>) {
    egui::Frame::new()
        .fill(colors().panel)
        .stroke(egui::Stroke::new(1.0, colors().border))
        .corner_radius(egui::CornerRadius::same(7))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_min_width((ui.available_width() - 28.0).max(160.0));
            let width = ui.available_width();
            if width < 640.0 {
                cpu_history_panel(ui, sample, history);
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);
                logical_processor_panel(ui, sample);
            } else {
                let gap = ui.spacing().item_spacing.x;
                let right_width = (width * 0.29).clamp(210.0, 430.0);
                let left_width = width - right_width - gap;
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(left_width, 183.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| cpu_history_panel(ui, sample, history),
                    );
                    let right = ui.allocate_ui_with_layout(
                        egui::vec2(right_width, 183.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| logical_processor_panel(ui, sample),
                    );
                    let x = right.response.rect.left() - gap / 2.0;
                    ui.painter().line_segment(
                        [
                            egui::pos2(x, right.response.rect.top() - 14.0),
                            egui::pos2(x, right.response.rect.bottom() + 14.0),
                        ],
                        egui::Stroke::new(1.0, colors().border.gamma_multiply(0.65)),
                    );
                });
            }
        });
}

fn cpu_history_panel(ui: &mut egui::Ui, sample: &Sample, history: &VecDeque<Option<f32>>) {
    ui.horizontal(|ui| {
        let (mark, _) = ui.allocate_exact_size(egui::vec2(12.0, 13.0), egui::Sense::hover());
        ui.painter()
            .circle_filled(mark.center(), 4.5, colors().series_b);
        ui.label(
            RichText::new("Processor activity")
                .size(13.0)
                .strong()
                .color(colors().text),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(format!("{:.0}% now", sample.cpu_usage))
                    .size(11.0)
                    .strong()
                    .color(colors().text),
            );
        });
    });
    ui.label(
        RichText::new(&sample.cpu_name)
            .size(10.5)
            .color(colors().muted),
    );
    cpu_history_chart(ui, history);
}

fn logical_processor_panel(ui: &mut egui::Ui, sample: &Sample) {
    ui.label(
        RichText::new("Per logical processor")
            .size(12.0)
            .strong()
            .color(colors().text),
    );
    ui.label(
        RichText::new(format!("{} logical processors", sample.cores.len()))
            .size(10.5)
            .color(colors().muted),
    );
    core_chart(ui, sample);
}

fn cpu_history_chart(ui: &mut egui::Ui, history: &VecDeque<Option<f32>>) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 143.0),
        egui::Sense::hover(),
    );
    let plot = egui::Rect::from_min_max(
        rect.min + egui::vec2(27.0, 5.0),
        rect.max - egui::vec2(5.0, 20.0),
    );
    let painter = ui.painter_at(rect);
    for level in [0, 25, 50, 75, 100] {
        let y = plot.bottom() - level as f32 / 100.0 * plot.height();
        painter.line_segment(
            [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
            egui::Stroke::new(1.0, colors().border.gamma_multiply(0.75)),
        );
        painter.text(
            egui::pos2(plot.left() - 6.0, y),
            egui::Align2::RIGHT_CENTER,
            level.to_string(),
            egui::FontId::proportional(9.0),
            colors().muted,
        );
    }
    for tick in 0..=9 {
        let x = plot.left() + tick as f32 / 9.0 * plot.width();
        painter.line_segment(
            [egui::pos2(x, plot.top()), egui::pos2(x, plot.bottom())],
            egui::Stroke::new(1.0, colors().border.gamma_multiply(0.75)),
        );
        let label = if tick == 9 {
            "Now".to_owned()
        } else {
            format!("{}s", 90 - tick * 10)
        };
        painter.text(
            egui::pos2(x, plot.bottom() + 11.0),
            if tick == 0 {
                egui::Align2::LEFT_CENTER
            } else if tick == 9 {
                egui::Align2::RIGHT_CENTER
            } else {
                egui::Align2::CENTER_CENTER
            },
            label,
            egui::FontId::proportional(9.0),
            colors().muted,
        );
    }
    let points: Vec<egui::Pos2> = history
        .iter()
        .enumerate()
        .filter_map(|(index, value)| {
            value.map(|value| {
                egui::pos2(
                    plot.right() - (history.len() - 1 - index) as f32 / 89.0 * plot.width(),
                    plot.bottom() - value / 100.0 * plot.height(),
                )
            })
        })
        .collect();
    let mut mesh = egui::Mesh::default();
    for pair in points.windows(2) {
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(pair[0], colors().series_a.gamma_multiply(0.28));
        mesh.colored_vertex(pair[1], colors().series_a.gamma_multiply(0.28));
        mesh.colored_vertex(
            egui::pos2(pair[1].x, plot.bottom()),
            colors().series_a.gamma_multiply(0.03),
        );
        mesh.colored_vertex(
            egui::pos2(pair[0].x, plot.bottom()),
            colors().series_a.gamma_multiply(0.03),
        );
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base, base + 2, base + 3);
    }
    painter.add(egui::Shape::mesh(mesh));
    if points.len() >= 2 {
        painter.add(egui::Shape::line(
            points,
            egui::Stroke::new(1.8, colors().series_a),
        ));
    }
}

fn network_card(
    ui: &mut egui::Ui,
    sample: &Sample,
    download_history: &VecDeque<Option<u64>>,
    upload_history: &VecDeque<Option<u64>>,
) {
    let connected = sample
        .network
        .adapters
        .iter()
        .filter(|adapter| adapter.connected)
        .count();
    card(
        ui,
        "Network traffic",
        &format!("{connected} connected · all interfaces"),
        colors().series_a,
        |ui| {
            main_metric(
                ui,
                &sample
                    .network
                    .download_bps
                    .map(rate)
                    .unwrap_or_else(|| "—".to_owned()),
                "download",
            );
            ui.label(
                RichText::new("Last 90 seconds")
                    .size(10.0)
                    .color(colors().muted),
            );
            network_chart(ui, download_history, upload_history);
            ui.horizontal(|ui| {
                ui.label(RichText::new("●").size(10.0).color(colors().series_a));
                ui.label(RichText::new("Download").size(10.5).color(colors().muted));
                ui.add_space(6.0);
                ui.label(RichText::new("●").size(10.0).color(colors().series_b));
                ui.label(RichText::new("Upload").size(10.5).color(colors().muted));
            });
            metric_row(
                ui,
                "Upload",
                &sample
                    .network
                    .upload_bps
                    .map(rate)
                    .unwrap_or_else(|| "Unavailable".to_owned()),
            );
            metric_row(ui, "Received", &network_bytes(sample.network.downloaded));
            metric_row(ui, "Sent", &network_bytes(sample.network.uploaded));
        },
    );
}

fn network_chart(
    ui: &mut egui::Ui,
    download: &VecDeque<Option<u64>>,
    upload: &VecDeque<Option<u64>>,
) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 103.0),
        egui::Sense::hover(),
    );
    let plot = rect.shrink2(egui::vec2(4.0, 8.0));
    let painter = ui.painter_at(rect);
    let peak = download
        .iter()
        .chain(upload.iter())
        .flatten()
        .copied()
        .max()
        .unwrap_or(0);
    let scale = peak.max(1) as f32;
    for fraction in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let y = plot.bottom() - fraction * plot.height();
        painter.line_segment(
            [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
            egui::Stroke::new(1.0, colors().border.gamma_multiply(0.75)),
        );
    }
    painter.text(
        plot.left_top() + egui::vec2(5.0, 3.0),
        egui::Align2::LEFT_TOP,
        rate(peak),
        egui::FontId::proportional(9.0),
        colors().muted,
    );
    if peak == 0 && download.iter().all(Option::is_none) {
        painter.text(
            plot.center(),
            egui::Align2::CENTER_CENTER,
            "Waiting for network counters",
            egui::FontId::proportional(11.0),
            colors().muted,
        );
    }
    for (history, color) in [(download, colors().series_a), (upload, colors().series_b)] {
        let mut previous = None;
        for (index, value) in history.iter().enumerate() {
            let current = value.map(|value| {
                let x = plot.right() - (history.len() - 1 - index) as f32 / 89.0 * plot.width();
                let y = plot.bottom() - value as f32 / scale * plot.height();
                egui::pos2(x, y)
            });
            if let (Some(start), Some(end)) = (previous, current) {
                painter.line_segment([start, end], egui::Stroke::new(1.8, color));
            }
            previous = current;
        }
    }
}

fn core_chart(ui: &mut egui::Ui, sample: &Sample) {
    let cores = &sample.cores;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 143.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    let plot = egui::Rect::from_min_max(
        rect.min + egui::vec2(2.0, 5.0),
        rect.max - egui::vec2(2.0, 20.0),
    );
    if cores.is_empty() {
        painter.text(
            plot.center(),
            egui::Align2::CENTER_CENTER,
            "Core readings unavailable",
            egui::FontId::proportional(11.0),
            colors().muted,
        );
        return;
    }
    let slot = plot.width() / cores.len() as f32;
    let bar_width = (slot * 0.60).clamp(2.0, 18.0);
    for (index, usage) in cores.iter().enumerate() {
        let x = plot.left() + slot * (index as f32 + 0.5);
        let hit = egui::Rect::from_min_max(
            egui::pos2(x - slot / 2.0, plot.top()),
            egui::pos2(x + slot / 2.0, rect.bottom()),
        );
        let response = ui.interact(
            hit,
            ui.scope_id().with(("logical-processor", index)),
            egui::Sense::hover(),
        );
        if response.hovered() {
            painter.rect_filled(
                hit.shrink2(egui::vec2(1.0, 3.0)),
                4.0,
                colors().series_b.gamma_multiply(0.10),
            );
        }
        let track = egui::Rect::from_min_max(
            egui::pos2(x - bar_width / 2.0, plot.top()),
            egui::pos2(x + bar_width / 2.0, plot.bottom()),
        );
        painter.rect_filled(track, 2.0, colors().border);
        let height = usage.clamp(0.0, 100.0) / 100.0 * plot.height();
        let fill = egui::Rect::from_min_max(
            egui::pos2(track.left(), plot.bottom() - height),
            track.right_bottom(),
        );
        painter.rect_filled(fill, 2.0, colors().series_b);
        if cores.len() <= 24 || index % 2 == 0 {
            painter.text(
                egui::pos2(x, plot.bottom() + 10.0),
                egui::Align2::CENTER_CENTER,
                format!("{}", index + 1),
                egui::FontId::proportional(9.0),
                colors().muted,
            );
        }
        response.on_hover_ui_at_pointer(|ui| {
            let detail = sample.core_details.get(index);
            egui::Frame::new()
                .fill(colors().panel)
                .stroke(egui::Stroke::new(1.0, colors().border))
                .corner_radius(6)
                .inner_margin(egui::Margin::same(11))
                .show(ui, |ui| {
                    ui.set_width(190.0);
                    ui.label(
                        RichText::new(format!("Logical processor {}", index + 1))
                            .size(12.0)
                            .strong()
                            .color(colors().text),
                    );
                    if let Some(name) = detail.and_then(|detail| detail.physical_core.as_deref()) {
                        ui.label(RichText::new(name).size(10.0).color(colors().series_b));
                    }
                    ui.add_space(5.0);
                    ui.separator();
                    ui.add_space(4.0);
                    metric_row(ui, "Usage", &format!("{usage:.0}%"));
                    metric_row(
                        ui,
                        "Clock",
                        &detail
                            .and_then(|detail| detail.clock_mhz)
                            .map(|mhz| format!("{mhz} MHz"))
                            .unwrap_or_else(|| "Unavailable".to_owned()),
                    );
                    let temperature = detail.and_then(|detail| detail.temperature);
                    let label = if temperature.is_some()
                        && detail.is_some_and(|detail| detail.package_temperature)
                    {
                        "Package temp"
                    } else if temperature.is_some() {
                        "Core temp"
                    } else {
                        "Temperature"
                    };
                    metric_row(ui, label, &reading(temperature, " °C"));
                });
        });
    }
}

fn gpu_card(ui: &mut egui::Ui, sample: &Sample, history: &VecDeque<Option<f32>>) {
    let gpu = sample.gpus.first();
    let title = gpu
        .map(|gpu| gpu.name.as_str())
        .unwrap_or("No NVIDIA GPU detected");
    card(ui, "Graphics load", title, colors().series_b, |ui| {
        main_metric(
            ui,
            &gpu.and_then(|gpu| gpu.usage)
                .map(|value| format!("{value:.0}%"))
                .unwrap_or_else(|| "—".to_owned()),
            "GPU usage",
        );
        ui.label(
            RichText::new("Last 90 seconds")
                .size(10.0)
                .color(colors().muted),
        );
        sparkline(ui, history, colors().series_b);
        metric_row(
            ui,
            "Temperature",
            &reading(gpu.and_then(|gpu| gpu.temp), " °C"),
        );
        metric_row(
            ui,
            "Power draw",
            &reading(gpu.and_then(|gpu| gpu.power), " W"),
        );
        metric_row(
            ui,
            "Graphics clock",
            &gpu.and_then(|gpu| gpu.clock_mhz)
                .map(|value| format!("{value} MHz"))
                .unwrap_or_else(|| "Unavailable".to_owned()),
        );
        metric_row(
            ui,
            "VRAM",
            &gpu.and_then(|gpu| gpu.vram_used.zip(gpu.vram_total))
                .map(|(used, total)| format!("{} / {}", bytes(used), bytes(total)))
                .unwrap_or_else(|| "Unavailable".to_owned()),
        );
    });
}

fn ram_card(ui: &mut egui::Ui, sample: &Sample, history: &VecDeque<Option<f32>>) {
    card(
        ui,
        "Physical memory",
        "Used by Windows and applications",
        colors().series_a,
        |ui| {
            main_metric(ui, &bytes(sample.ram_used), "used");
            ui.label(
                RichText::new("Last 90 seconds")
                    .size(10.0)
                    .color(colors().muted),
            );
            sparkline(ui, history, colors().series_a);
            metric_row(
                ui,
                "In use",
                &format!("{:.0}%", percent(sample.ram_used, sample.ram_total)),
            );
            metric_row(ui, "Available", &bytes(sample.ram_available));
            metric_row(ui, "Installed", &bytes(sample.ram_total));
        },
    );
}

fn drives_card(ui: &mut egui::Ui, sample: &Sample) {
    let primary = sample
        .disks
        .iter()
        .find(|drive| drive.mount.eq_ignore_ascii_case("C:\\"))
        .or_else(|| sample.disks.first());
    let subtitle = primary
        .map(|drive| format!("{}  {}", drive.mount, drive.name))
        .unwrap_or_else(|| "No mounted volume".to_owned());
    card(ui, "Disk space", &subtitle, colors().storage, |ui| {
        if let Some(drive) = primary {
            let used = drive.total.saturating_sub(drive.free);
            storage_ring(ui, percent(used, drive.total));
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(format!("{} free", bytes(drive.free)))
                        .size(20.0)
                        .strong()
                        .color(colors().text),
                );
                ui.label(
                    RichText::new(format!(
                        "{} used of {} total",
                        bytes(used),
                        bytes(drive.total)
                    ))
                    .size(10.5)
                    .color(colors().muted),
                );
            });
            ui.add_space(10.0);
            metric_row(
                ui,
                "Read",
                &drive
                    .read_bps
                    .map(rate)
                    .unwrap_or_else(|| "Unavailable".to_owned()),
            );
            metric_row(
                ui,
                "Write",
                &drive
                    .write_bps
                    .map(rate)
                    .unwrap_or_else(|| "Unavailable".to_owned()),
            );
        } else {
            ui.label(RichText::new("No drive readings available").color(colors().muted));
        }
    });
}

fn sensor_card(ui: &mut egui::Ui, sample: &Sample, tab: &mut Tab) {
    egui::Frame::new().fill(colors().panel).stroke(egui::Stroke::new(1.0, colors().border))
        .corner_radius(egui::CornerRadius::same(7)).inner_margin(egui::Margin::same(15))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Hardware sensors").size(13.0).strong().color(colors().text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(if sample.sensor_online { "Provider connected" } else { "Optional provider offline" }).size(11.0).color(if sample.sensor_online { colors().success } else { colors().muted }));
                });
            });
            ui.add_space(4.0);
            if !sample.sensor_online {
                ui.label(RichText::new("The bundled sensor bridge is starting, or this hardware does not expose additional sensors. Supported temperatures, fans, power, and drive sensors appear here.").size(11.0).color(colors().muted));
            } else {
                ui.label(RichText::new(format!("{} additional readings are reporting", sample.sensors.len())).size(11.0).color(colors().muted));
            }
            ui.add_space(8.0);
            if ui.button("Browse all sensors").clicked() {
                *tab = Tab::Sensors;
            }
        });
}

fn sensor_table(
    ui: &mut egui::Ui,
    readings: &[Reading],
    stats: &mut StatsBook,
    search: &mut String,
) {
    egui::Frame::new()
        .fill(colors().panel)
        .stroke(egui::Stroke::new(1.0, colors().border))
        .corner_radius(egui::CornerRadius::same(7))
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("All sensors")
                            .size(22.0)
                            .strong()
                            .color(colors().text),
                    );
                    ui.label(
                        RichText::new("Current, minimum, maximum, and average since launch")
                            .size(12.0)
                            .color(colors().muted),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Reset history").clicked() {
                        stats.clear();
                    }
                });
            });
            ui.add_space(16.0);
            ui.add(
                egui::TextEdit::singleline(search)
                    .hint_text("Search readings or hardware")
                    .desired_width(330.0),
            );
            ui.add_space(16.0);
            let query = search.trim().to_ascii_lowercase();
            let mut last_group = "";
            egui::ScrollArea::horizontal().show(ui, |ui| {
                egui::Grid::new("readings_grid")
                    .num_columns(5)
                    .spacing([12.0, 5.0])
                    .show(ui, |ui| {
                        table_cell(ui, "Reading", 300.0, colors().muted, true);
                        for heading in ["Current", "Min", "Max", "Average"] {
                            table_cell(ui, heading, 118.0, colors().muted, true);
                        }
                        ui.end_row();
                        for reading in readings {
                            if !query.is_empty()
                                && !reading.name.to_ascii_lowercase().contains(&query)
                                && !reading.group.to_ascii_lowercase().contains(&query)
                            {
                                continue;
                            }
                            if reading.group != last_group {
                                table_cell(ui, &reading.group, 300.0, colors().series_a, true);
                                for _ in 0..4 {
                                    ui.label("");
                                }
                                ui.end_row();
                                last_group = &reading.group;
                            }
                            table_cell(ui, &reading.name, 300.0, colors().text, false);
                            table_cell(ui, &reading.formatted(), 118.0, colors().text, true);
                            let stat = stats.get(&reading.key);
                            for value in [
                                stat.map(|s| s.min),
                                stat.map(|s| s.max),
                                stat.map(|s| s.mean()),
                            ] {
                                let text = value
                                    .map(|value| reading.format_value(value))
                                    .unwrap_or_else(|| "—".to_owned());
                                table_cell(ui, &text, 118.0, colors().muted, false);
                            }
                            ui.end_row();
                        }
                    });
            });
        });
}

fn table_cell(ui: &mut egui::Ui, text: &str, width: f32, color: Color32, strong: bool) {
    let mut value = RichText::new(text).size(11.5).color(color);
    if strong {
        value = value.strong();
    }
    ui.add_sized(
        [width, 21.0],
        egui::Label::new(value).halign(egui::Align::Min).truncate(),
    );
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 770.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Perfm",
        options,
        Box::new(|_| Ok(Box::new(PerfmApp::new()))),
    )
}

#[cfg(test)]
mod tests {
    use super::{Sensor, cpu_core_clock, preferred_sensor};

    #[test]
    fn cpu_clock_fallback_averages_valid_core_readings() {
        let sensors = vec![
            Sensor {
                name: "P-Core #1".to_owned(),
                sensor_type: "Clock".to_owned(),
                value: Some(4230.4),
                parent: "/intelcpu/0".to_owned(),
            },
            Sensor {
                name: "E-Core #1".to_owned(),
                sensor_type: "Clock".to_owned(),
                value: Some(3299.6),
                parent: "/intelcpu/0".to_owned(),
            },
            Sensor {
                name: "Bus Speed".to_owned(),
                sensor_type: "Clock".to_owned(),
                value: Some(9999.0),
                parent: "/intelcpu/0".to_owned(),
            },
        ];
        assert_eq!(cpu_core_clock(&sensors), Some(3765));
    }

    #[test]
    fn package_metric_does_not_use_core_sensor() {
        let sensors = vec![Sensor {
            name: "CPU Cores".to_owned(),
            sensor_type: "Power".to_owned(),
            value: Some(14.0),
            parent: "/intelcpu/0".to_owned(),
        }];
        assert_eq!(preferred_sensor(&sensors, "Power", &["CPU Package"]), None);
    }

    #[test]
    fn zero_cpu_package_power_is_unavailable() {
        let sensors = vec![super::Sensor {
            name: "CPU Package".to_owned(),
            sensor_type: "Power".to_owned(),
            value: Some(0.0),
            parent: "/intelcpu/0".to_owned(),
        }];
        assert_eq!(
            super::preferred_sensor(&sensors, "Power", &["CPU Package"]),
            None
        );
    }
}
