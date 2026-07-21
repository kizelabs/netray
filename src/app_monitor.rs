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

extern "C" {
    fn proc_pidpath(
        pid: i32,
        buffer: *mut std::os::raw::c_void,
        buffersize: u32,
    ) -> i32;
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
        // (name_pid is "NAME.PID"; PID is recovered later for name resolution)
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
    // deltas up to a friendly app name so an app's helper processes sum into
    // one row (e.g. "Google Chrome Helper" folds into "Google Chrome").
    let mut by_name: HashMap<String, (u64, u64)> = HashMap::new();
    let mut name_cache: HashMap<i32, String> = HashMap::new();
    for (key, (in2, out2)) in last {
        let Some((in1, out1)) = first.get(key) else { continue };
        let din = in2.saturating_sub(*in1);
        let dout = out2.saturating_sub(*out1);
        if din == 0 && dout == 0 {
            continue;
        }
        let (raw, pid) = split_key(key);
        let display = match pid {
            Some(pid) => name_cache
                .entry(pid)
                .or_insert_with(|| friendly_name(pid, raw))
                .clone(),
            None => cleanup(raw),
        };
        let entry = by_name.entry(display).or_insert((0, 0));
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
    // Sort by download first, then upload, so the download column reads as a
    // clean descending list.
    apps.sort_by(|a, b| {
        b.recv_speed
            .cmp(&a.recv_speed)
            .then(b.sent_speed.cmp(&a.sent_speed))
    });
    apps.truncate(max);
    apps
}

/// "Google Chrome H.5231" -> ("Google Chrome H", Some(5231)).
fn split_key(key: &str) -> (&str, Option<i32>) {
    if let Some(idx) = key.rfind('.') {
        if idx > 0 && idx + 1 < key.len() {
            if let Ok(pid) = key[idx + 1..].parse::<i32>() {
                return (&key[..idx], Some(pid));
            }
        }
    }
    (key, None)
}

/// Resolve a human-friendly app name for a PID. Prefers the enclosing
/// `.app` bundle name from the executable path (which also collapses helper
/// processes into their parent app), then the executable's basename, and
/// finally the raw nettop name — each run through `cleanup`.
fn friendly_name(pid: i32, raw: &str) -> String {
    if let Some(path) = proc_path(pid) {
        if let Some(app) = app_bundle_name(&path) {
            return app;
        }
        if let Some(base) = path.rsplit('/').next() {
            let cleaned = cleanup(base);
            if !cleaned.is_empty() {
                return cleaned;
            }
        }
    }
    cleanup(raw)
}

/// Full executable path for a PID via libproc, or None.
fn proc_path(pid: i32) -> Option<String> {
    // PROC_PIDPATHINFO_MAXSIZE is 4 * MAXPATHLEN (4096).
    let mut buf = vec![0u8; 4096];
    let n = unsafe { proc_pidpath(pid, buf.as_mut_ptr() as *mut _, buf.len() as u32) };
    if n <= 0 {
        return None;
    }
    Some(String::from_utf8_lossy(&buf[..n as usize]).into_owned())
}

/// "/Applications/Google Chrome.app/Contents/MacOS/..." -> "Google Chrome".
fn app_bundle_name(path: &str) -> Option<String> {
    let idx = path.find(".app/")?;
    path[..idx].rsplit('/').next().map(|s| s.to_string())
}

/// Turn a raw process name into something presentable: drop a parenthetical
/// suffix ("Google Chrome Helper (Renderer)"), and if it looks like a
/// reverse-DNS id ("com.apple.Safari") keep the last, capitalized component.
fn cleanup(name: &str) -> String {
    let name = name.split(" (").next().unwrap_or(name).trim();
    if name.contains('.') && !name.contains(' ') {
        if let Some(last) = name.rsplit('.').next() {
            let mut chars = last.chars();
            if let Some(first) = chars.next() {
                return first.to_uppercase().collect::<String>() + chars.as_str();
            }
        }
    }
    name.to_string()
}
