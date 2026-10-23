using System.Text.Json;
using LibreHardwareMonitor.Hardware;

var computer = new Computer
{
    IsCpuEnabled = true,
    IsGpuEnabled = true,
    IsMemoryEnabled = true,
    IsMotherboardEnabled = true,
    IsStorageEnabled = true,
    IsBatteryEnabled = true,
};

try
{
    computer.Open();
    while (true)
    {
        var readings = new List<SensorReading>();
        foreach (var hardware in computer.Hardware)
            Collect(hardware, readings);
        Console.WriteLine(JsonSerializer.Serialize(readings));
        Thread.Sleep(2000);
    }
}
catch (Exception error)
{
    Console.Error.WriteLine(error);
    return 1;
}
finally
{
    computer.Close();
}

static void Collect(IHardware hardware, List<SensorReading> readings)
{
    hardware.Update();
    foreach (var sensor in hardware.Sensors)
    {
        var value = sensor.Value;
        readings.Add(new SensorReading(
            sensor.Name,
            sensor.SensorType.ToString(),
            value.HasValue && float.IsFinite(value.Value) ? value : null,
            hardware.Identifier.ToString()));
    }
    foreach (var child in hardware.SubHardware)
        Collect(child, readings);
}

record SensorReading(string name, string sensor_type, float? value, string parent);
