use std::collections::BTreeMap;
use std::ffi::c_void;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetSystemCpuSetInformation(
        buffer: *mut u8,
        buffer_length: u32,
        returned_length: *mut u32,
        process: *mut c_void,
        flags: u32,
    ) -> i32;
}

#[derive(Clone, Copy)]
struct CpuSet {
    group: u16,
    logical: u8,
    core: u8,
    efficiency: u8,
}

fn map_sensor_cores(sets: &[CpuSet], logical_count: usize) -> Vec<Option<String>> {
    let mut names = vec![None; logical_count];
    let mut cores: BTreeMap<u8, Vec<CpuSet>> = BTreeMap::new();
    for set in sets
        .iter()
        .copied()
        .filter(|set| set.group == 0 && usize::from(set.logical) < logical_count)
    {
        cores.entry(set.core).or_default().push(set);
    }
    let mut cores: Vec<Vec<CpuSet>> = cores.into_values().collect();
    cores.sort_by_key(|core| core.iter().map(|set| set.logical).min());
    let min_efficiency = cores
        .iter()
        .flat_map(|core| core.iter())
        .map(|set| set.efficiency)
        .min();
    let max_efficiency = cores
        .iter()
        .flat_map(|core| core.iter())
        .map(|set| set.efficiency)
        .max();
    let hybrid = min_efficiency != max_efficiency;
    let mut performance_number = 0;
    let mut efficient_number = 0;
    for (generic_number, core) in cores.iter().enumerate() {
        let efficiency = core[0].efficiency;
        let name = if hybrid && Some(efficiency) == max_efficiency {
            performance_number += 1;
            format!("P-Core #{performance_number}")
        } else if hybrid {
            efficient_number += 1;
            format!("E-Core #{efficient_number}")
        } else {
            format!("Core #{}", generic_number + 1)
        };
        for set in core {
            names[usize::from(set.logical)] = Some(name.clone());
        }
    }
    names
}

pub fn sensor_names(logical_count: usize) -> Vec<Option<String>> {
    let mut required = 0u32;
    // SAFETY: The first call only asks Windows for the required buffer length.
    unsafe {
        GetSystemCpuSetInformation(
            std::ptr::null_mut(),
            0,
            &mut required,
            std::ptr::null_mut(),
            0,
        );
    }
    if required == 0 {
        return vec![None; logical_count];
    }
    let mut buffer = vec![0u8; required as usize];
    // SAFETY: Windows writes at most `buffer.len()` bytes into the owned buffer.
    let ok = unsafe {
        GetSystemCpuSetInformation(
            buffer.as_mut_ptr(),
            required,
            &mut required,
            std::ptr::null_mut(),
            0,
        )
    };
    if ok == 0 {
        return vec![None; logical_count];
    }
    let mut sets = Vec::new();
    let mut offset = 0;
    while offset + 20 <= buffer.len() {
        let record = &buffer[offset..];
        let size = u32::from_le_bytes(record[0..4].try_into().unwrap()) as usize;
        if size < 20 || offset + size > buffer.len() {
            break;
        }
        let kind = u32::from_le_bytes(record[4..8].try_into().unwrap());
        if kind == 0 {
            sets.push(CpuSet {
                group: u16::from_le_bytes(record[12..14].try_into().unwrap()),
                logical: record[14],
                core: record[15],
                efficiency: record[18],
            });
        }
        offset += size;
    }
    map_sensor_cores(&sets, logical_count)
}

#[cfg(test)]
mod tests {
    use super::{CpuSet, map_sensor_cores};

    #[test]
    fn maps_smt_siblings_to_the_same_physical_core_sensor() {
        let sets = [
            CpuSet {
                group: 0,
                logical: 0,
                core: 0,
                efficiency: 1,
            },
            CpuSet {
                group: 0,
                logical: 1,
                core: 0,
                efficiency: 1,
            },
            CpuSet {
                group: 0,
                logical: 2,
                core: 2,
                efficiency: 1,
            },
            CpuSet {
                group: 0,
                logical: 3,
                core: 3,
                efficiency: 0,
            },
            CpuSet {
                group: 0,
                logical: 4,
                core: 4,
                efficiency: 0,
            },
        ];
        assert_eq!(
            map_sensor_cores(&sets, 5),
            vec![
                Some("P-Core #1".to_owned()),
                Some("P-Core #1".to_owned()),
                Some("P-Core #2".to_owned()),
                Some("E-Core #1".to_owned()),
                Some("E-Core #2".to_owned()),
            ]
        );
    }
}
