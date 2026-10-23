use crate::Sensor;

pub fn parse_sensor_line(line: &str) -> Result<Vec<Sensor>, serde_json::Error> {
    serde_json::from_str(line)
}

#[cfg(test)]
mod tests {
    use super::parse_sensor_line;

    #[test]
    fn bridge_readings_preserve_parent_type_and_missing_values() {
        let line = r#"[
            {"name":"CPU Package","sensor_type":"Temperature","value":67.25,"parent":"/intelcpu/0"},
            {"name":"CPU Package","sensor_type":"Power","value":19.5,"parent":"/intelcpu/0"},
            {"name":"Core 1","sensor_type":"Temperature","value":null,"parent":"/intelcpu/0"}
        ]"#;
        let sensors = parse_sensor_line(line).unwrap();
        assert_eq!(sensors.len(), 3);
        assert_eq!(sensors[0].parent, "/intelcpu/0");
        assert_eq!(sensors[0].name, "CPU Package");
        assert_eq!(sensors[0].sensor_type, "Temperature");
        assert_eq!(sensors[0].value, Some(67.25));
        assert_eq!(sensors[1].value, Some(19.5));
        assert_eq!(sensors[2].value, None);
    }
}
