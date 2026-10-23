# Perfm

A native Windows performance monitor written in Rust. It samples once per second and keeps the interface idle between updates. The dark overview has four telemetry cards for drive, memory, graphics, and network activity. The processor panel pairs a 90-second total usage chart with per-logical-processor bars. Hover a logical processor bar to see its usage, current clock, and physical-core temperature. If only package temperature is available, the hover card labels it as such. The navigation rail opens a searchable All sensors table with current, minimum, maximum, and average readings for the current Perfm session.

The Settings icon in the navigation rail opens 13 color themes: Perfm Original and the 12 palettes from the color-palettes collection. Selecting a theme updates the interface immediately. Perfm saves the choice in `%LOCALAPPDATA%\Perfm\settings.json` and restores it on the next launch. Each palette keeps the dashboard dark and uses its supplied neutral and accent shades.

## Readings

- CPU: total and per-logical-processor usage, current Windows processor frequency, temperature and package power when a sensor provider exposes them.
- RAM: used, available, and total physical memory.
- Drives: free and total space, read and write rates, and activity for mounted volumes, plus any temperature sensors exposed by LibreHardwareMonitor.
- Network: live download and upload rates across all interfaces, a 90-second traffic history, session transfer totals, and per-adapter rates in All sensors. Rates use the actual time between samples.
- NVIDIA GPU: utilization, memory, temperature, power draw, and graphics clock through NVML when the driver supports each reading.
- Other hardware sensors: temperatures, powers, fans, voltages, and additional memory and drive readings through the bundled LibreHardwareMonitorLib bridge.

The interface shows `Unavailable` for a reading that Windows, the driver, or the optional sensor provider does not expose. A missing reading is never represented as zero. Drive performance counters and optional hardware sensors are queried on separate threads so they cannot stall the main dashboard. Cached disk rates expire if their source stops updating. The status badge changes to `STALE` if dashboard sampling stops.

## Build and run

Install the current stable Rust toolchain and Visual Studio C++ Build Tools, open the **x64 Native Tools Command Prompt for VS 2022**, then run:

```powershell
cargo run --release
```

The executable will be at `target\release\perfm.exe`. To make the complete package, install the .NET 10 SDK and run `scripts\package.ps1`. The package script builds the small .NET bridge and places everything in `dist\Perfm-Integrated`. Launch `dist\Perfm-Integrated\Perfm.exe`; the bundled bridge starts and stops with Perfm. This framework-dependent package requires the .NET 10 runtime on the laptop.

## Bundled CPU and drive sensors

The package includes [LibreHardwareMonitorLib 0.9.6](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor) through NuGet. The bridge sends readings to Perfm through a local process pipe. It does not start the LibreHardwareMonitor window or web server. License and third-party notices are in `dist\Perfm-Integrated\SensorBridge`.

Sensor availability depends on the laptop firmware, drivers, and access level. On this laptop, the unelevated bridge reports many load, memory, voltage, battery, and GPU readings. It cannot open the running PawnIO driver (`Access denied`, Windows error 5), so Intel CPU package temperature and core clocks are unavailable and package power reports zero. Double-click `Run Perfm with CPU sensors.cmd` in the package to request administrator access for Perfm and its bundled bridge. Windows will show a UAC prompt. This is optional; normal launch still works for sensors that do not need that access. Perfm displays a missing package reading as `Unavailable` instead of presenting zero watts as a real measurement. The CPU clock tile reads the current Windows `Processor Frequency` counter once per second; if that counter is unavailable, it displays the average of valid LibreHardwareMonitor core clocks. The NVIDIA GPU readings use NVIDIA's driver API directly.


## HWiNFO reference

This design was informed by an inspection of the installed HWiNFO sensor window, especially its hardware grouping and current/minimum/maximum/average view. Perfm is an independent implementation and does not require HWiNFO to run. See [the design notes](docs/design.md) for what was observed and what remains unknown.
