# Bundled LibreHardwareMonitor sensor helper

## Intent

Perfm should show supported temperature, fan, voltage, and power readings without asking the user to separately download LibreHardwareMonitor. The helper starts with Perfm and its sensor feed stays on this laptop.

## Constraints and approach

LibreHardwareMonitor 0.9.6 no longer publishes the WMI feed that Perfm currently reads. Bundle LibreHardwareMonitorLib 0.9.6 with a small .NET 10 bridge that sends sensor readings to Perfm through a private process pipe. Include its MPL 2.0 license and third-party notices. Perfm starts and stops the bridge with its window. Other Perfm readings continue when the bridge or a hardware sensor is unavailable.

The selected approach uses the sensor library directly and avoids the full LibreHardwareMonitor GUI and HTTP server. The official 0.9.6 app's HTTP server cannot bind to loopback because of an upstream bug and would expose readings to the network. A separate app installation would not meet the requested package behavior.

## Verification

Parse representative bridge messages including missing values, build and run Perfm, verify actual readings and process cleanup. Some low-level sensors may require an approved hardware driver and remain unavailable if the laptop does not expose them.
