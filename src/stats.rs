use std::collections::HashMap;

#[derive(Default)]
pub struct StatsBook {
    values: HashMap<String, Stats>,
}

#[derive(Clone, Copy)]
pub struct Stats {
    pub min: f64,
    pub max: f64,
    sum: f64,
    pub count: u64,
}

impl Stats {
    pub fn mean(self) -> f64 {
        self.sum / self.count as f64
    }
}

impl StatsBook {
    pub fn record(&mut self, key: &str, value: f64) {
        if !value.is_finite() {
            return;
        }
        self.values
            .entry(key.to_owned())
            .and_modify(|stats| {
                stats.min = stats.min.min(value);
                stats.max = stats.max.max(value);
                stats.sum += value;
                stats.count += 1;
            })
            .or_insert(Stats {
                min: value,
                max: value,
                sum: value,
                count: 1,
            });
    }

    pub fn get(&self, key: &str) -> Option<&Stats> {
        self.values.get(key)
    }

    pub fn clear(&mut self) {
        self.values.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::StatsBook;

    #[test]
    fn tracks_minimum_maximum_and_mean_for_a_sensor() {
        let mut book = StatsBook::default();
        book.record("cpu/package/power", 12.0);
        book.record("cpu/package/power", 18.0);
        book.record("cpu/package/power", 9.0);

        let stats = book.get("cpu/package/power").unwrap();
        assert_eq!(stats.min, 9.0);
        assert_eq!(stats.max, 18.0);
        assert_eq!(stats.mean(), 13.0);
        assert_eq!(stats.count, 3);
    }

    #[test]
    fn sensors_do_not_share_history() {
        let mut book = StatsBook::default();
        book.record("cpu/usage", 10.0);
        book.record("gpu/usage", 80.0);

        assert_eq!(book.get("cpu/usage").unwrap().max, 10.0);
        assert_eq!(book.get("gpu/usage").unwrap().min, 80.0);
    }

    #[test]
    fn reset_clears_the_session() {
        let mut book = StatsBook::default();
        book.record("cpu/usage", 20.0);
        book.clear();
        assert!(book.get("cpu/usage").is_none());
    }
}
