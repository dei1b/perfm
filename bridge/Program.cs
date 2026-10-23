using System.Text.Json;
using LibreHardwareMonitor.Hardware;

var interval = new SensorInterval(ParseInterval(args.Length > 0 ? args[0] : null));
var inputThread = new Thread(() =>
{
    string? line;
    while ((line = Console.ReadLine()) is not null)
    {
        if (TryParseInterval(line, out var seconds))
        {
            Volatile.Write(ref interval.Seconds, seconds);
            interval.Changed.Set();
        }
    }
}) { IsBackground = true, Name = "perfm-interval-input" };
inputThread.Start();

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
    var readings = new List<SensorReading>(160);
    var sensorTypeNames = new Dictionary<SensorType, string>();
    while (true)
    {
        readings.Clear();
        foreach (var hardware in computer.Hardware)
            Collect(hardware, readings, sensorTypeNames);
        Console.WriteLine(JsonSerializer.Serialize(readings, SensorJsonContext.Default.ListSensorReading));
        interval.Changed.WaitOne(TimeSpan.FromSeconds(Math.Max(2, Volatile.Read(ref interval.Seconds))));
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

static void Collect(IHardware hardware, List<SensorReading> readings, Dictionary<SensorType, string> sensorTypeNames)
{
    hardware.Update();
    var parent = hardware.Identifier.ToString();
    foreach (var sensor in hardware.Sensors)
    {
        if (sensor.SensorType == SensorType.Timing ||
            sensor.Name.EndsWith("Distance to TjMax", StringComparison.OrdinalIgnoreCase))
            continue;

        var value = sensor.Value;
        if (!sensorTypeNames.TryGetValue(sensor.SensorType, out var sensorTypeName))
        {
            sensorTypeName = sensor.SensorType.ToString();
            sensorTypeNames.Add(sensor.SensorType, sensorTypeName);
        }
        readings.Add(new SensorReading(
            sensor.Name,
            sensorTypeName,
            value.HasValue && float.IsFinite(value.Value) ? value : null,
            parent));
    }
    foreach (var child in hardware.SubHardware)
        Collect(child, readings, sensorTypeNames);
}

static bool TryParseInterval(string? text, out int seconds)
{
    if (int.TryParse(text, out seconds) && seconds is 1 or 2 or 5 or 10 or 30 or 60 or 300)
        return true;
    seconds = 1;
    return false;
}

static int ParseInterval(string? text) => TryParseInterval(text, out var seconds) ? seconds : 1;

readonly record struct SensorReading(string name, string sensor_type, float? value, string parent);

sealed class SensorInterval(int seconds)
{
    public int Seconds = seconds;
    public AutoResetEvent Changed { get; } = new(false);
}
