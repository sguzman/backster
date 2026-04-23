pub struct AsciiReporter;

impl AsciiReporter {
    pub fn sparkline(values: &[f64]) -> String {
        let chars = [" ", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
        if values.is_empty() {
            return String::new();
        }
        let min = values.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max = values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
        let range = max - min;

        values.iter()
            .map(|&v| {
                let i = if range == 0.0 {
                    0
                } else {
                    ((v - min) / range * (chars.len() - 1) as f64).round() as usize
                };
                chars[i]
            })
            .collect()
    }
}
