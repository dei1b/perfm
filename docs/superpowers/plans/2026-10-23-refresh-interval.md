# Refresh Interval Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Let the user choose 1, 2, 5, 10, or 30 second updates, plus 1 or 5 minute updates, in Settings while preserving the chosen rate across launches.

**Architecture:** Persist the rate beside the theme in `settings.json`. A shared Rust schedule drives dashboard, CPU clock, and disk polling; the bundled sensor helper receives rate changes over stdin and updates its own timer. The UI redraws when a sample arrives and labels history using the selected rate.

**Tech Stack:** Rust, egui/eframe, .NET 10, LibreHardwareMonitorLib.

**Spec:** User request in this chat on 2026-10-23.

## Global Constraints

- Keep 1 second as the default for existing settings files.
- Retain the current theme and all remaining sensor readings.
- Keep slower hardware provider minimums of 2 seconds for LibreHardwareMonitor and 3 seconds for disk WMI.

## Review Focus

- A legacy settings file containing only `theme` loads with a 1 second interval.
- Changing from 30 seconds to 1 second wakes collectors without waiting 30 seconds.
- The header does not show `Stale` during a healthy 30 second interval.
- Chart labels reflect 90 samples at the selected interval.
- The sensor helper honors a live interval update and keeps producing valid JSON.

---

## Tasks

- [x] Add failing tests for settings compatibility and the shared schedule, then implement both.
- [x] Wire the selected rate into Rust collectors, freshness checks, repainting, and chart labels; run Rust tests.
- [x] Add the Settings selector and preserve both settings fields on either change.
- [x] Add the sensor helper stdin protocol and verify its live cadence with the packaged executable.
- [x] Build, package, launch the app with a saved interval, and document the options.
