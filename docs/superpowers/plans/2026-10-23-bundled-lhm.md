# Bundled LibreHardwareMonitor Implementation Plan

**Goal:** Ship Perfm with an automatically started LibreHardwareMonitor helper for additional hardware sensors.

**Architecture:** Perfm starts a local .NET bridge using LibreHardwareMonitorLib, reads JSON lines from its stdout on the sensor thread, and stops the bridge with its window. No HTTP server is used.

**Tech Stack:** Rust, serde_json, .NET 10, LibreHardwareMonitorLib 0.9.6.

**Spec:** `docs/superpowers/specs/2026-10-23-bundled-lhm-design.md`

## Tasks

- [x] Add a failing test for bridge sensor values, including null readings.
- [x] Implement JSON line parsing and pipe sampling; run Rust tests.
- [x] Build the bundled .NET sensor bridge and verify live readings.
- [x] Package the bridge, LibreHardwareMonitorLib, and license notices; update the UI guidance and README.
- [x] Build release, run Perfm, inspect the readings and UI, and verify the final distribution.
