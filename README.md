# Perfm

Perfm is a lightweight, native Windows performance monitor written in Rust. It
provides a focused dashboard for CPU, memory, storage, network, and graphics
telemetry, with a searchable sensor table for detailed readings.

The Settings icon in the navigation rail opens 13 color themes: Perfm Original and the 12 palettes from the color-palettes collection. Selecting a theme updates the interface immediately. Perfm saves the choice in `%LOCALAPPDATA%\Perfm\settings.json` and restores it on the next launch. Each palette keeps the dashboard dark and uses its supplied neutral and accent shades.

Perfm samples the dashboard once per second, keeps the interface responsive
while slower hardware queries run in the background, and does not require an
account, telemetry service, or background daemon.

## Features

- **CPU:** total usage, per-logical-processor usage, current processor
  frequency, package temperature, and package power when available.
- **Memory:** used, available, and installed physical memory.
- **Storage:** capacity, read/write throughput, activity, and available drive
  temperature sensors for mounted volumes.
- **Network:** aggregate and per-adapter download/upload rates, a 90-second
  traffic history, and session transfer totals.
- **NVIDIA graphics:** utilization, VRAM usage, temperature, power draw, and
  graphics clock through NVML when supported by the installed driver.
- **Additional sensors:** temperatures, power, fans, voltages, and other
  hardware readings through the bundled LibreHardwareMonitor bridge.
- **Focused sensor list:** per-core `Distance to TjMax` diagnostics and RAM
  timing rows are omitted; actual temperatures and capacity remain.
- **Sensor history:** current, minimum, maximum, and average values in the
  searchable **All sensors** view.
- **Explicit unavailable data:** readings that are not exposed by Windows,
  firmware, drivers, or an optional provider are shown as `Unavailable`, not
  as a misleading zero.

## Requirements

Perfm is currently supported on **64-bit Windows**.

### Running the application

- A packaged Perfm build.
- No .NET runtime is needed for the default NativeAOT sensor bridge package.
  The optional managed bridge package requires the .NET 10 runtime.
- An NVIDIA driver is required only for NVIDIA-specific telemetry.

### Building from source

- The current stable Rust toolchain.
- Visual Studio 2022 C++ Build Tools, including the Windows development tools.
- The .NET 10 SDK if you want to build the sensor bridge and create a complete
  distributable package.

## Quick start

Download or build a package, then launch:

```text
dist\Perfm-Integrated\Perfm.exe
```

The packaged executable starts the sensor bridge from the adjacent
`SensorBridge` directory when it is present. Core Windows, storage, network,
and NVML readings can still work when optional hardware sensors are
unavailable.

To request administrator access for sensors that require it, run:

```text
dist\Perfm-Integrated\Run Perfm with CPU sensors.cmd
```

Windows displays a UAC prompt. Elevated access is optional; it may expose
additional CPU temperature, clock, power, or motherboard readings on systems
where the hardware provider requires it.

## Build from source

Open an **x64 Native Tools Command Prompt for VS 2022**, then run:

```powershell
cargo build --release
```

The Rust executable is written to:

```text
target\release\perfm.exe
```

You can also run the application directly during development:

```powershell
cargo run --release
```

### Create a distributable package

Build the Rust executable first, install the .NET 10 SDK, and run:

```powershell
.\scripts\package.ps1
```

The script builds the .NET sensor bridge and creates:

```text
dist\Perfm-Integrated\
├── Perfm.exe
├── Run Perfm with CPU sensors.cmd
└── SensorBridge\
    ├── Perfm.SensorBridge.exe
    └── license and third-party notices
```

The default package compiles the sensor bridge to native code, reducing its
working memory and removing the target-machine .NET runtime requirement. To
choose another output directory:

```powershell
.\scripts\package.ps1 -OutputDirectory C:\Releases\Perfm
```

If the native bridge is incompatible with a particular sensor provider, build
the framework-dependent bridge instead (this requires the .NET 10 runtime on
the target machine):

```powershell
.\scripts\package.ps1 -BridgeMode Managed -OutputDirectory C:\Releases\Perfm-Managed
```

## How it works

Perfm combines several data sources:

| Area | Source |
| --- | --- |
| CPU usage and system information | Windows and `sysinfo` |
| Processor frequency | Windows `Processor Frequency` performance counter |
| Drive capacity | `sysinfo` |
| Drive throughput and activity | Windows logical disk performance counters |
| Network rates and totals | Windows interface counters |
| NVIDIA telemetry | NVIDIA Management Library (NVML) |
| Optional temperatures, power, fans, and voltages | LibreHardwareMonitorLib 0.9.6 through the bundled .NET bridge |
| UI and rendering | Rust with `eframe`/`egui` |

The bridge emits sensor snapshots over a local process pipe. It does not start
the LibreHardwareMonitor window or web server. Disk performance collection and
optional hardware sensor collection run independently from the dashboard so a
slow or unavailable provider does not block the UI. Cached disk rates expire
when their source stops updating, and the status indicator changes to `STALE`
if dashboard sampling stops.

## Sensor availability

Sensor availability varies by Windows version, firmware, driver, hardware, and
whether Perfm is running elevated. In particular:

- Some CPU package and per-core sensors require administrator access or a
  provider driver.
- NVIDIA readings require a compatible NVIDIA driver and may be unavailable on
  unsupported hardware.
- Drive temperature and SMART readings depend on what the device and provider
  expose.
- The processor clock tile prefers the Windows processor-frequency counter and
  falls back to an average of valid hardware-provider core clocks.

Perfm does not treat an unavailable sensor as a zero measurement. This keeps
missing data distinguishable from a real zero value.

## Project layout

```text
src/       Rust application and collectors
bridge/    .NET sensor bridge using LibreHardwareMonitorLib
scripts/   Packaging and elevated-launch helpers
licenses/  Third-party license and notice files
docs/      Design documentation
```

## License

Perfm is licensed under the MIT License; see [LICENSE](LICENSE).

The packaged sensor bridge includes LibreHardwareMonitorLib under its own
license. See the files in [`licenses/`](licenses/) for the MPL-2.0 license and
third-party notices.
