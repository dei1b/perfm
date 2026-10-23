# Perfm design

## Goal

Build a personal Windows laptop performance monitor with a modern, readable interface and low idle cost. The app should make CPU, GPU, RAM, drive, temperature, and power readings easy to scan, with more detailed sensor data available on demand.

## HWiNFO reference inspection

- **Observed in the running app:** HWiNFO64 v8.22-5670 had a Sensors Status window with hardware groups and columns for Current, Minimum, Maximum, and Average. Visible groups included Intel PCH and ACPI temperatures, an NVMe SMART section, drive activity and read/write rates, and detailed RTX 3050 temperature, power, clocks, load, and memory readings.
- **Observed in the installed artifact:** `C:\Program Files\HWiNFO64\HWiNFO64.exe` is a 64-bit PE executable with SHA-256 `136af1c5097d2bd04e1195c929124ce8545d16170d7b6e7ae6f13feb61e3c0ae` (REA `open_binary`). Native decompilation was unavailable because no Ghidra, IDA, Hopper, or Kuna provider was configured. REA's resource parser rejected this file's resource range.
- **Inference:** A focused overview plus a searchable full sensor table preserves the useful scanning and historical comparison workflow without copying HWiNFO's dense layout.
- **Unknown:** The original app's internal data collection, refresh policy, and exact sensor mapping were not recovered. This project does not reuse its code or bundle its binary.

## Product decisions

- Native Rust and egui desktop app. The user-supplied monitoring screenshots guide a near-black canvas, narrow navigation rail, four telemetry panels, restrained color coding, compact numeric tiles, and a processor panel with a wide total usage chart beside logical processor bars. Perfm uses its own layout and live readings.
- Palette: canvas `#08090B`, rail `#0C0D10`, panel `#111215`, border `#313339`, text `#F2F3F5`, with mint storage, blue memory, violet graphics, and amber CPU accents. Thin dividers and square-leaning corners make dense readings easy to scan.
- Appearance settings provide Perfm Original plus Pastel & Soft, Neutral & Earthy, Bold & High Contrast, and Minimalist & Modern. Each new palette uses its specified background, surface, border, and three accent colors; no dashboard layout changes with theme selection. The selected theme is stored in the user's local app data and restored at startup. Removed theme selections fall back to Perfm Original.
- Refresh settings offer 1, 2, 5, 10, 30, 60, and 300 second dashboard samples. The rate is stored beside the theme, shared by the collectors, and sent to the hardware sensor helper while it runs. Provider minimum rates remain 2 seconds for LibreHardwareMonitor and 3 seconds for disk WMI. Chart time labels use the selected rate.
- Overview shows primary drive capacity, physical memory, NVIDIA graphics, network traffic, quick CPU/GPU readings, and true per-logical-processor load. All sensors retains search and current/minimum/maximum/average columns. Chart labels describe the available 90-second session history; unavailable hardware data remains explicit.
- Overview for CPU, NVIDIA GPU, RAM, and drives. Separate sensor table with Current, Min, Max, and Average columns, search, and hardware grouping.
- A sampler thread polls at the selected refresh interval and requests a repaint when a new sample arrives. Storage enumeration and hardware providers retain their own minimum intervals. The sampling path shares sensor frames and avoids repeated metadata work where possible.
- Core readings use `sysinfo`, Windows logical disk performance counters, and NVIDIA NVML. Optional LibreHardwareMonitor WMI readings fill hardware-specific temperature, power, fan, and voltage gaps.
- Missing readings display as unavailable. HWiNFO is a reference for visible workflows; Perfm runs without it.
- No account, telemetry upload, or background service.

## Verification

- Build and test on this Windows laptop.
- Inspect the running interface at normal and narrow window sizes.
- Confirm live CPU, RAM, drive, and NVIDIA GPU values, and confirm optional sensor gaps are labeled.
- Check sampling and UI process usage after startup.
