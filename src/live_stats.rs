//! Lightweight live system readings for the animated home-page logo.
//! Reads only /proc (no subprocesses), so sampling once a second on the UI
//! thread costs microseconds.

use std::time::Instant;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Reading {
    /// Busy share of all CPUs since the previous sample, 0.0..=1.0.
    pub cpu: f64,
    pub mem_used_kib: u64,
    pub mem_total_kib: u64,
    /// Bytes per second received + sent on non-loopback interfaces.
    pub net_bytes_per_sec: f64,
}

impl Reading {
    pub fn mem_fraction(&self) -> f64 {
        if self.mem_total_kib == 0 {
            0.0
        } else {
            (self.mem_used_kib as f64 / self.mem_total_kib as f64).clamp(0.0, 1.0)
        }
    }
}

pub struct Sampler {
    cpu: Option<(u64, u64)>,
    net: Option<(u64, Instant)>,
    pub latest: Reading,
}

impl Sampler {
    pub fn new() -> Self {
        let mut sampler = Self { cpu: None, net: None, latest: Reading::default() };
        sampler.sample();
        sampler
    }

    pub fn sample(&mut self) -> Reading {
        if let Some((busy, total)) = std::fs::read_to_string("/proc/stat")
            .ok()
            .and_then(|text| parse_cpu(&text))
        {
            if let Some((last_busy, last_total)) = self.cpu
                && total > last_total
            {
                self.latest.cpu =
                    (busy.saturating_sub(last_busy) as f64 / (total - last_total) as f64).clamp(0.0, 1.0);
            }
            self.cpu = Some((busy, total));
        }
        if let Some((used, total)) = std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|text| parse_meminfo(&text))
        {
            self.latest.mem_used_kib = used;
            self.latest.mem_total_kib = total;
        }
        if let Some(bytes) = std::fs::read_to_string("/proc/net/dev")
            .ok()
            .map(|text| parse_net_bytes(&text))
        {
            let now = Instant::now();
            if let Some((last, at)) = self.net {
                let seconds = now.duration_since(at).as_secs_f64();
                if seconds > 0.0 {
                    self.latest.net_bytes_per_sec = bytes.saturating_sub(last) as f64 / seconds;
                }
            }
            self.net = Some((bytes, now));
        }
        self.latest
    }
}

/// (busy, total) jiffies from the aggregate `cpu` line.
pub fn parse_cpu(text: &str) -> Option<(u64, u64)> {
    let line = text.lines().find(|line| line.starts_with("cpu "))?;
    let values: Vec<u64> = line.split_whitespace().skip(1).filter_map(|v| v.parse().ok()).collect();
    if values.len() < 4 {
        return None;
    }
    // user nice system idle iowait irq softirq steal (guest is already in user).
    let total: u64 = values.iter().take(8).sum();
    let idle = values[3] + values.get(4).copied().unwrap_or(0);
    Some((total - idle, total))
}

/// (used, total) KiB, where used = MemTotal - MemAvailable.
pub fn parse_meminfo(text: &str) -> Option<(u64, u64)> {
    let field = |name: &str| {
        text.lines()
            .find(|line| line.starts_with(name))
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
    };
    let total = field("MemTotal:")?;
    let available = field("MemAvailable:")?;
    Some((total.saturating_sub(available), total))
}

/// Total rx + tx bytes over every interface except loopback.
pub fn parse_net_bytes(text: &str) -> u64 {
    text.lines()
        .skip(2)
        .filter_map(|line| line.split_once(':'))
        .filter(|(name, _)| name.trim() != "lo")
        .map(|(_, data)| {
            let fields: Vec<u64> = data.split_whitespace().filter_map(|v| v.parse().ok()).collect();
            fields.first().copied().unwrap_or(0) + fields.get(8).copied().unwrap_or(0)
        })
        .sum()
}

pub fn format_rate(bytes_per_sec: f64) -> String {
    match bytes_per_sec {
        b if b >= 1_048_576.0 => format!("{:.1} MB/s", b / 1_048_576.0),
        b if b >= 1024.0 => format!("{:.0} KB/s", b / 1024.0),
        b => format!("{b:.0} B/s"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_proc_files() {
        let stat = "cpu  100 0 50 800 50 0 0 0 0 0\ncpu0 1 2 3 4\n";
        assert_eq!(parse_cpu(stat), Some((150, 1000)));
        assert_eq!(parse_cpu("garbage"), None);
        let mem = "MemTotal:       16000000 kB\nMemFree: 1 kB\nMemAvailable:   10000000 kB\n";
        assert_eq!(parse_meminfo(mem), Some((6_000_000, 16_000_000)));
        let net = "Inter-|   Receive\n face |bytes\n    lo: 999 1 0 0 0 0 0 0 999 1 0 0 0 0 0 0\n  eth0: 1000 1 0 0 0 0 0 0 500 1 0 0 0 0 0 0\n";
        assert_eq!(parse_net_bytes(net), 1500);
        assert_eq!(format_rate(2_097_152.0), "2.0 MB/s");
        assert_eq!(format_rate(2048.0), "2 KB/s");
        assert_eq!(format_rate(12.0), "12 B/s");
        let reading = Reading { mem_used_kib: 4, mem_total_kib: 16, ..Default::default() };
        assert_eq!(reading.mem_fraction(), 0.25);
        assert_eq!(Reading::default().mem_fraction(), 0.0);
    }
}
