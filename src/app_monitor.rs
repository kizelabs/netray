//! Per-process network usage via macOS's built-in `nettop`.
//!
//! sysinfo only exposes per-interface counters, so for a per-app breakdown we
//! shell out to `nettop`, which reports cumulative bytes_in/bytes_out per
//! process and needs no elevated privileges. Running it with `-l 2 -s 1` emits
//! two snapshots one second apart; the per-process difference is the ~1s rate.
//! That call blocks ~1s (plus nettop startup), so it must run off the UI path.

use std::collections::HashMap;
use std::process::Command;

#[derive(Clone, Debug)]
pub struct AppUsage {
    pub name: String,
    /// bytes/sec downloaded (nettop bytes_in)
    pub recv_speed: u64,
    /// bytes/sec uploaded (nettop bytes_out)
    pub sent_speed: u64,
}

/// Sample the top `max` processes by current network throughput.
/// Returns an empty vec if nettop is unavailable or produced nothing usable.
pub fn sample_top_apps(max: usize) -> Vec<AppUsage> {
    let output = Command::new("nettop")
        .args([
            "-P", // per-process (aggregate the process's sockets)
            "-x", // plain values, no human formatting
            "-J", "bytes_in,bytes_out", // only the columns we need
            "-l", "2", // two samples...
            "-s", "1", // ...one second apart
        ])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    let text = String::from_utf8_lossy(&output.stdout);

    // nettop prefixes each sample with a header line beginning "time".
    let mut samples: Vec<HashMap<String, (u64, u64)>> = Vec::new();
    let mut current: Option<HashMap<String, (u64, u64)>> = None;
    for line in text.lines() {
        if line.trim_start().starts_with("time") {
            if let Some(m) = current.take() {
                samples.push(m);
            }
            current = Some(HashMap::new());
            continue;
        }
        let Some(map) = current.as_mut() else { continue };
        // Layout: "<timestamp> <NAME.PID> ... <bytes_in> <bytes_out>".
        // NAME can contain spaces ("Google Chrome H"), so anchor on the ends:
        // first token is the timestamp, last two are the byte counters, and
        // everything between is "NAME.PID".
        let toks: Vec<&str> = line.split_whitespace().collect();
        if toks.len() < 4 {
            continue;
        }
        let bytes_out: u64 = toks[toks.len() - 1].parse().unwrap_or(0);
        let bytes_in: u64 = toks[toks.len() - 2].parse().unwrap_or(0);
        let name_pid = toks[1..toks.len() - 2].join(" ");
        map.insert(name_pid, (bytes_in, bytes_out));
    }
    if let Some(m) = current.take() {
        samples.push(m);
    }
    if samples.len() < 2 {
        return Vec::new();
    }
    let first = &samples[0];
    let last = &samples[samples.len() - 1];

    // Diff per (name.pid) so distinct PIDs don't alias, then aggregate the
    // deltas up to the display name so an app's helpers sum into one row.
    let mut by_name: HashMap<String, (u64, u64)> = HashMap::new();
    for (key, (in2, out2)) in last {
        let Some((in1, out1)) = first.get(key) else { continue };
        let din = in2.saturating_sub(*in1);
        let dout = out2.saturating_sub(*out1);
        if din == 0 && dout == 0 {
            continue;
        }
        let entry = by_name.entry(strip_pid(key)).or_insert((0, 0));
        entry.0 += din;
        entry.1 += dout;
    }

    // Drop sub-kilobyte-per-second chatter: it rounds to "0K/s" in the UI and
    // just fills the list with rows that read as idle. "Actively consuming"
    // means at least a displayable kilobyte a second.
    const MIN_BYTES_PER_SEC: u64 = 1024;
    let mut apps: Vec<AppUsage> = by_name
        .into_iter()
        .map(|(name, (din, dout))| AppUsage {
            name,
            recv_speed: din,
            sent_speed: dout,
        })
        .filter(|a| a.recv_speed + a.sent_speed >= MIN_BYTES_PER_SEC)
        .collect();
    apps.sort_by(|a, b| (b.recv_speed + b.sent_speed).cmp(&(a.recv_speed + a.sent_speed)));
    apps.truncate(max);
    apps
}

/// "Google Chrome H.5231" -> "Google Chrome H". Only strips a trailing
/// ".<digits>" so a name that happens to end in a dot-word is left alone.
fn strip_pid(key: &str) -> String {
    if let Some(idx) = key.rfind('.') {
        if idx > 0 && key[idx + 1..].chars().all(|c| c.is_ascii_digit()) && idx + 1 < key.len() {
            return key[..idx].to_string();
        }
    }
    key.to_string()
}
