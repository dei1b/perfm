# Perfm design

## Goal

Build a personal Windows laptop performance monitor with a modern, readable interface and low idle cost. The app should make CPU, GPU, RAM, drive, temperature, and power readings easy to scan, with more detailed sensor data available on demand.

## HWiNFO reference inspection

- **Observed in the running app:** HWiNFO64 v8.22-5670 had a Sensors Status window with hardware groups and columns for Current, Minimum, Maximum, and Average. Visible groups included Intel PCH and ACPI temperatures, an NVMe SMART section, drive activity and read/write rates, and detailed RTX 3050 temperature, power, clocks, load, and memory readings.
- **Observed in the installed artifact:** `C:\Program Files\HWiNFO64\HWiNFO64.exe` is a 64-bit PE executable with SHA-256 `136af1c5097d2bd04e1195c929124ce8545d16170d7b6e7ae6f13feb61e3c0ae` (REA `open_binary`). Native decompilation was unavailable because no Ghidra, IDA, Hopper, or Kuna provider was configured. REA's resource parser rejected this file's resource range.
- **Inference:** A focused overview plus a searchable full sensor table preserves the useful scanning and historical comparison workflow without copying HWiNFO's dense layout.
- **Unknown:** The original app's internal data collection, refresh policy, and exact sensor mapping were not recovered. This project does not reuse its code or bundle its binary.

## Product decisions

- Native Rust and egui desktop app. The user-supplied monitoring screenshot guides a near-black canvas, narrow navigation rail, three prominent telemetry panels, restrained color coding, compact numeric tiles, and a full-width processor chart. Perfm uses its own layout and live readings.
- Palette: canvas `#08090B`, rail `#0C0D10`, panel `#111215`, border `#313339`, text `#F2F3F5`, with mint storage, blue memory, violet graphics, and amber CPU accents. Thin dividers and square-leaning corners make dense readings easy to scan.
- Overview shows primary drive capacity, physical memory, NVIDIA graphics, quick CPU/GPU readings, and true per-logical-processor load. All sensors retains search and current/minimum/maximum/average columns. Chart labels describe the available 90-second session history; unavailable hardware data remains explicit.
- Overview for CPU, NVIDIA GPU, RAM, and drives. Separate sensor table with Current, Min, Max, and Average columns, search, and hardware grouping.
- A sampler thread polls once per second. The UI repaints once per second, and storage enumeration and WMI sensor queries use slower intervals.
- Core readings use `sysinfo`, Windows logical disk performance counters, and NVIDIA NVML. Optional LibreHardwareMonitor WMI readings fill hardware-specific temperature, power, fan, and voltage gaps.
- Missing readings display as unavailable. HWiNFO is a reference for visible workflows; Perfm runs without it.
- No account, telemetry upload, or background service.

## Verification

- Build and test on this Windows laptop.
- Inspect the running interface at normal and narrow window sizes.
- Confirm live CPU, RAM, drive, and NVIDIA GPU values, and confirm optional sensor gaps are labeled.
- Check sampling and UI process usage after startup.
