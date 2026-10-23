using System.Text.Json.Serialization;

[JsonSerializable(typeof(List<SensorReading>))]
internal partial class SensorJsonContext : JsonSerializerContext;
