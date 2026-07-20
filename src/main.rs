mod icon;
mod monitor;
mod tray_title;

use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use bytesize::ByteSize;
use monitor::{InterfaceStats, NetworkMonitor};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    TrayIconBuilder,
};
use winit::event_loop::{EventLoop, EventLoopProxy};

const REFRESH_INTERVAL: Duration = Duration::from_millis(1000);

#[derive(Debug, Clone)]
enum UserEvent {
    Menu(tray_icon::menu::MenuEvent),
    Refresh,
}

#[derive(Debug, Clone, Default)]
struct NetworkState {
    recv_speed: u64,
    sent_speed: u64,
    total_recv: u64,
    total_sent: u64,
    interfaces: Vec<(String, InterfaceStats)>,
}

#[derive(Default)]
struct AppState {
    peak_recv: u64,
    peak_sent: u64,
    network: NetworkState,
}

impl AppState {
    fn update(&mut self, snapshot: &NetworkState) {
        if snapshot.recv_speed > self.peak_recv {
            self.peak_recv = snapshot.recv_speed;
        }
        if snapshot.sent_speed > self.peak_sent {
            self.peak_sent = snapshot.sent_speed;
        }
        self.network = snapshot.clone();
    }
}

struct MenuItems {
    quit: MenuItem,
    reset_totals: MenuItem,
    interfaces_sub: Submenu,
    recv_speed: MenuItem,
    sent_speed: MenuItem,
    total_recv: MenuItem,
    total_sent: MenuItem,
    peak_recv: MenuItem,
    peak_sent: MenuItem,
}

fn build_menu() -> Result<(Menu, MenuItems)> {
    let menu = Menu::new();
    let header = MenuItem::new("NeTray — Network Monitor", false, None);
    menu.append(&header)?;
    menu.append(&PredefinedMenuItem::separator())?;

    let recv_speed = MenuItem::new("↓ 0 B/s", false, None);
    let sent_speed = MenuItem::new("↑ 0 B/s", false, None);
    let total_recv = MenuItem::new("Session ↓ 0 B", false, None);
    let total_sent = MenuItem::new("Session ↑ 0 B", false, None);
    let peak_recv = MenuItem::new("Peak ↓ 0 B/s", false, None);
    let peak_sent = MenuItem::new("Peak ↑ 0 B/s", false, None);
    let speeds_sub = Submenu::new("Bandwidth", true);
    speeds_sub.append(&recv_speed)?;
    speeds_sub.append(&sent_speed)?;
    speeds_sub.append(&PredefinedMenuItem::separator())?;
    speeds_sub.append(&total_recv)?;
    speeds_sub.append(&total_sent)?;
    speeds_sub.append(&PredefinedMenuItem::separator())?;
    speeds_sub.append(&peak_recv)?;
    speeds_sub.append(&peak_sent)?;
    menu.append(&speeds_sub)?;

    let interfaces_sub = Submenu::new("Interfaces", true);
    interfaces_sub.append(&MenuItem::new("Scanning…", false, None))?;
    menu.append(&interfaces_sub)?;

    menu.append(&PredefinedMenuItem::separator())?;

    let reset_totals = MenuItem::new("Reset Peak Speeds", true, None);
    menu.append(&reset_totals)?;
    menu.append(&PredefinedMenuItem::separator())?;
    let quit = MenuItem::new("Quit NeTray", true, None);
    menu.append(&quit)?;

    let items = MenuItems {
        quit,
        reset_totals,
        interfaces_sub,
        recv_speed,
        sent_speed,
        total_recv,
        total_sent,
        peak_recv,
        peak_sent,
    };
    Ok((menu, items))
}

fn main() -> Result<()> {
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let (menu, items) = build_menu()?;

    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::Menu(event));
    }));

    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("NeTray")
        .with_icon(icon::blank_icon())
        .build()?;

    let _ = tray_icon.set_icon(None);

    let (tx, rx) = mpsc::channel::<NetworkState>();
    spawn_timer_thread(event_loop.create_proxy());
    spawn_monitor_thread(tx);

    let mut state = AppState::default();
    let mut last_ui = Instant::now() - REFRESH_INTERVAL;

    #[allow(deprecated)]
    event_loop.run(move |event, elwt| {
        match event {
            winit::event::Event::UserEvent(UserEvent::Menu(menu_event)) => {
                if menu_event.id() == items.quit.id() {
                    elwt.exit();
                } else if menu_event.id() == items.reset_totals.id() {
                    state.peak_recv = 1;
                    state.peak_sent = 1;
                    apply_to_ui(&state, &items, &tray_icon);
                }
            }
            winit::event::Event::UserEvent(UserEvent::Refresh) => {
                if let Ok(snapshot) = rx.try_recv() {
                    state.update(&snapshot);
                }
            }
            winit::event::Event::AboutToWait => {
                if last_ui.elapsed() >= REFRESH_INTERVAL {
                    last_ui = Instant::now();
                    apply_to_ui(&state, &items, &tray_icon);
                }
            }
            _ => {}
        }
    })?;

    Ok(())
}

fn apply_to_ui(state: &AppState, items: &MenuItems, tray: &tray_icon::TrayIcon) {
    let _ = items.recv_speed.set_text(&format!("↓ {}/s", ByteSize(state.network.recv_speed)));
    let _ = items.sent_speed.set_text(&format!("↑ {}/s", ByteSize(state.network.sent_speed)));
    let _ = items.total_recv.set_text(&format!("Session ↓ {}", ByteSize(state.network.total_recv)));
    let _ = items.total_sent.set_text(&format!("Session ↑ {}", ByteSize(state.network.total_sent)));
    let _ = items.peak_recv.set_text(&format!("Peak ↓ {}/s", ByteSize(state.peak_recv)));
    let _ = items.peak_sent.set_text(&format!("Peak ↑ {}/s", ByteSize(state.peak_sent)));

    rebuild_interfaces_submenu(&items.interfaces_sub, &state.network.interfaces);

    let recv_str = format_speed_bytes(state.network.recv_speed);
    let sent_str = format_speed_bytes(state.network.sent_speed);

    // Upload on top, download below. Each line is padded out to TITLE_COLS on the
    // trailing edge so the block keeps a constant width (and so never shoves the
    // neighbouring menu bar icons around) while staying flush left.
    tray_title::set_two_line_title(
        &pad_title(&format!("↑ {}/s", sent_str)),
        &pad_title(&format!("↓ {}/s", recv_str)),
    );

    let tooltip = format!(
        "NeTray\n↓ {}/s  ↑ {}/s",
        ByteSize(state.network.recv_speed),
        ByteSize(state.network.sent_speed)
    );
    let _ = tray.set_tooltip(Some(&tooltip));
}

/// Widest line the title can produce: "↑ 1.00G/s".
const TITLE_COLS: usize = 9;

/// Pads to TITLE_COLS with U+2007 FIGURE SPACE rather than a plain space —
/// AppKit trims trailing ASCII whitespace when measuring a button title, which
/// would collapse the padding and defeat the whole point.
fn pad_title(s: &str) -> String {
    let n = s.chars().count();
    let mut out = String::from(s);
    for _ in n..TITLE_COLS {
        out.push('\u{2007}');
    }
    out
}

fn format_speed_bytes(bps: u64) -> String {
    let k = bps as f64 / 1024.0;
    if k < 1.0 {
        "0K".to_string()
    } else if k < 10.0 {
        format!("{:.1}K", k)
    } else if k < 1024.0 {
        format!("{:.0}K", k)
    } else if k < 1024.0 * 10.0 {
        format!("{:.1}M", k / 1024.0)
    } else if k < 1024.0 * 1024.0 {
        format!("{:.0}M", k / 1024.0)
    } else {
        format!("{:.2}G", k / 1024.0 / 1024.0)
    }
}

fn rebuild_interfaces_submenu(sub: &Submenu, interfaces: &[(String, InterfaceStats)]) {
    let existing = sub.items();
    for kind in &existing {
        match kind {
            tray_icon::menu::MenuItemKind::MenuItem(mi) => {
                let _ = sub.remove(mi);
            }
            tray_icon::menu::MenuItemKind::Check(mi) => {
                let _ = sub.remove(mi);
            }
            tray_icon::menu::MenuItemKind::Icon(mi) => {
                let _ = sub.remove(mi);
            }
            tray_icon::menu::MenuItemKind::Predefined(mi) => {
                let _ = sub.remove(mi);
            }
            tray_icon::menu::MenuItemKind::Submenu(s) => {
                let _ = sub.remove(s);
            }
        }
    }
    if interfaces.is_empty() {
        let _ = sub.append(&MenuItem::new("No active interfaces", false, None));
    } else {
        for (name, stats) in interfaces {
            let label = format!(
                "{}  ↓ {}/s  ↑ {}/s   {} B",
                short_name(name),
                ByteSize(stats.recv_speed),
                ByteSize(stats.sent_speed),
                ByteSize(stats.total_received + stats.total_transmitted)
            );
            let _ = sub.append(&MenuItem::new(&label, false, None));
        }
    }
}

fn short_name(name: &str) -> &str {
    if let Some(idx) = name.find(':') {
        &name[..idx]
    } else {
        name
    }
}

fn spawn_timer_thread(proxy: EventLoopProxy<UserEvent>) {
    thread::spawn(move || {
        loop {
            thread::sleep(REFRESH_INTERVAL);
            if proxy.send_event(UserEvent::Refresh).is_err() {
                break;
            }
        }
    });
}

fn spawn_monitor_thread(tx: Sender<NetworkState>) {
    thread::spawn(move || {
        let mut monitor = NetworkMonitor::new();
        loop {
            monitor.refresh();
            let interfaces = monitor.active_interfaces();
            let state = NetworkState {
                recv_speed: monitor.total_recv_speed,
                sent_speed: monitor.total_sent_speed,
                total_recv: monitor.total_recv,
                total_sent: monitor.total_sent,
                interfaces: interfaces
                    .into_iter()
                    .map(|(n, s)| (n.clone(), s.clone()))
                    .collect(),
            };
            if tx.send(state).is_err() {
                break;
            }
            thread::sleep(REFRESH_INTERVAL);
        }
    });
}