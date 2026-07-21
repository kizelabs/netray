#![cfg(target_os = "macos")]
//! Renders the "Active apps" rows as native NSMenuItems with attributed
//! titles, which muda's plain-text menu items cannot do. This buys us three
//! things the request needs: real per-value colors, tab-stop columns (name
//! left, download / upload right-aligned), and a compact monospaced font.
//!
//! We reach the underlying NSMenu via muda's `ContextMenu::ns_menu()` and
//! insert/remove our own items around muda's static ones. muda calls
//! `setAutoenablesItems(false)`, so our rows can stay enabled (hence
//! full-strength color) without being clickable actions.

use std::cell::RefCell;
use std::ffi::c_void;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::ClassType;
use objc2::{class, msg_send};
use objc2_app_kit::{
    NSFontAttributeName, NSForegroundColorAttributeName, NSMenu, NSMenuItem,
    NSMutableParagraphStyle, NSParagraphStyleAttributeName, NSRightTextAlignment,
};
use objc2_foundation::{MainThreadMarker, NSRange, NSString};

use crate::app_monitor::AppUsage;

// Native rows currently in the menu, kept so we can remove them next refresh.
// UI-thread only, hence thread_local rather than a lock.
thread_local! {
    static ROWS: RefCell<Vec<Retained<NSMenuItem>>> = const { RefCell::new(Vec::new()) };
}

const ROW_FONT_SIZE: f64 = 12.0;
/// Right edges (points from the item's left) of the two number columns. Both
/// the header labels and the values right-align to these, so the columns line
/// up. Sized to clear a ~24-char name plus the "Download" header label.
const TAB_DOWN: f64 = 250.0;
const TAB_UP: f64 = 330.0;
/// Max app-name length before ellipsizing.
const NAME_MAX: usize = 24;

/// Rebuild the app rows in `ns_menu` (a `*mut NSMenu` from `Menu::ns_menu()`),
/// inserting them starting at `base_index`, just after the section header.
pub fn render(ns_menu: *mut c_void, base_index: usize, apps: &[AppUsage]) {
    if ns_menu.is_null() {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    unsafe {
        let menu: &NSMenu = &*(ns_menu as *const NSMenu);

        ROWS.with(|rows| {
            for item in rows.borrow().iter() {
                menu.removeItem(item);
            }
            rows.borrow_mut().clear();
        });

        let dark = app_is_dark();
        // The header uses the same tab-stop columns as the rows, so "Download"
        // / "Upload" sit directly above their values.
        let mut new_rows: Vec<Retained<NSMenuItem>> = vec![make_header(mtm)];
        if apps.is_empty() {
            let placeholder = "  (no active connections)";
            let len = placeholder.chars().count();
            new_rows.push(make_row(placeholder, &[(0, len, secondary_label())], true, mtm));
        } else {
            new_rows.extend(apps.iter().map(|a| make_app_row(a, dark, mtm)));
        }

        for (offset, item) in new_rows.iter().enumerate() {
            menu.insertItem_atIndex(item, (base_index + offset) as isize);
        }
        ROWS.with(|rows| *rows.borrow_mut() = new_rows);
    }
}

/// Column header, laid out on the same tab stops as the data rows.
unsafe fn make_header(mtm: MainThreadMarker) -> Retained<NSMenuItem> {
    let text = "App\tDownload\tUpload";
    let app_len = 3;
    let down_len = "Download".chars().count();
    let down_start = app_len + 1;
    let up_start = down_start + down_len + 1;
    let up_len = "Upload".chars().count();
    let colors = [
        (0, app_len, label()),
        (down_start, down_len, label()),
        (up_start, up_len, label()),
    ];
    // Disabled so it reads as a dimmed header and isn't highlightable.
    make_row(text, &colors, false, mtm)
}

unsafe fn make_app_row(app: &AppUsage, dark: bool, mtm: MainThreadMarker) -> Retained<NSMenuItem> {
    let name = truncate(&app.name, NAME_MAX);
    // No arrow or "/s": the columns are labeled by the header, and the unit
    // letter (K/M/G) already conveys magnitude.
    let down = crate::format_speed_bytes(app.recv_speed);
    let up = crate::format_speed_bytes(app.sent_speed);
    let text = format!("{}\t{}\t{}", name, down, up);

    // Column ranges, in UTF-16 units. Every glyph here is in the BMP (letters,
    // digits, tab), so char count == UTF-16 length.
    let name_len = name.chars().count();
    let down_start = name_len + 1; // past the '\t'
    let down_len = down.chars().count();
    let up_start = down_start + down_len + 1;
    let up_len = up.chars().count();

    let colors = [
        (0, name_len, label()),
        (down_start, down_len, magnitude_color(&down, dark)),
        (up_start, up_len, magnitude_color(&up, dark)),
    ];
    make_row(&text, &colors, true, mtm)
}

/// Build a menu item whose attributed title uses our monospaced font, the two
/// right-aligned tab stops, and the given per-range foreground colors.
/// `colors` is a slice of (utf16_location, utf16_length, NSColor*).
unsafe fn make_row(
    text: &str,
    colors: &[(usize, usize, *mut AnyObject)],
    enabled: bool,
    mtm: MainThreadMarker,
) -> Retained<NSMenuItem> {
    let ns_text = NSString::from_str(text);
    let total = text.chars().count();

    let font: *mut AnyObject = msg_send![
        class!(NSFont),
        monospacedSystemFontOfSize: ROW_FONT_SIZE, weight: 0.0f64
    ];

    let para = NSMutableParagraphStyle::init(NSMutableParagraphStyle::alloc());
    let tabs: *mut AnyObject = msg_send![class!(NSMutableArray), array];
    for loc in [TAB_DOWN, TAB_UP] {
        let empty: *mut AnyObject = msg_send![class!(NSDictionary), dictionary];
        let tab: *mut AnyObject = msg_send![class!(NSTextTab), alloc];
        let tab: *mut AnyObject = msg_send![
            tab, initWithTextAlignment: NSRightTextAlignment, location: loc, options: empty
        ];
        let _: () = msg_send![tabs, addObject: tab];
    }
    let _: () = msg_send![&*para, setTabStops: tabs];

    let attr: *mut AnyObject = msg_send![class!(NSMutableAttributedString), alloc];
    let attr: *mut AnyObject = msg_send![attr, initWithString: &*ns_text];
    let whole = NSRange::new(0, total);
    let _: () = msg_send![attr, addAttribute: NSFontAttributeName, value: font, range: whole];
    let _: () = msg_send![attr, addAttribute: NSParagraphStyleAttributeName, value: &*para, range: whole];
    for &(loc, len, color) in colors {
        if len == 0 || loc + len > total {
            continue;
        }
        let range = NSRange::new(loc, len);
        let _: () = msg_send![attr, addAttribute: NSForegroundColorAttributeName, value: color, range: range];
    }

    let empty_key = NSString::from_str("");
    let item = NSMenuItem::initWithTitle_action_keyEquivalent(
        mtm.alloc::<NSMenuItem>(),
        &empty_key,
        None,
        &empty_key,
    );
    let _: () = msg_send![&*item, setAttributedTitle: attr];
    // Data rows stay enabled so their colors render at full strength (muda
    // turned off auto-enable); the header passes false to read as dimmed.
    item.setEnabled(enabled);
    item
}

/// "849K" / "0K" -> a magnitude color. Ramps calm -> hot with the unit, and
/// dims to gray when the value is zero. Mirrors the menu bar title palette.
unsafe fn magnitude_color(s: &str, dark: bool) -> *mut AnyObject {
    let unit = s.chars().find(|c| matches!(c, 'K' | 'M' | 'G'));
    let is_zero = s == "0K";
    let (r, g, b) = match (unit, is_zero, dark) {
        (_, true, true) => (0.60, 0.62, 0.66),
        (_, true, false) => (0.42, 0.44, 0.48),
        (Some('K'), _, true) => (0.40, 0.88, 0.56),
        (Some('K'), _, false) => (0.06, 0.55, 0.24),
        (Some('M'), _, true) => (1.0, 0.80, 0.34),
        (Some('M'), _, false) => (0.72, 0.45, 0.0),
        (Some('G'), _, true) => (1.0, 0.47, 0.44),
        (Some('G'), _, false) => (0.78, 0.12, 0.12),
        (_, _, true) => (0.85, 0.87, 0.90),
        (_, _, false) => (0.20, 0.22, 0.25),
    };
    srgb(r, g, b)
}

unsafe fn srgb(r: f64, g: f64, b: f64) -> *mut AnyObject {
    msg_send![class!(NSColor), colorWithSRGBRed: r, green: g, blue: b, alpha: 1.0f64]
}

unsafe fn label() -> *mut AnyObject {
    msg_send![class!(NSColor), labelColor]
}

unsafe fn secondary_label() -> *mut AnyObject {
    msg_send![class!(NSColor), secondaryLabelColor]
}

/// Menu appearance follows the app's effective appearance; default to dark.
unsafe fn app_is_dark() -> bool {
    let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
    if app.is_null() {
        return true;
    }
    let appearance: *mut AnyObject = msg_send![app, effectiveAppearance];
    if appearance.is_null() {
        return true;
    }
    let name: *mut AnyObject = msg_send![appearance, name];
    if name.is_null() {
        return true;
    }
    let utf8: *const std::os::raw::c_char = msg_send![name, UTF8String];
    if utf8.is_null() {
        return true;
    }
    std::ffi::CStr::from_ptr(utf8)
        .to_string_lossy()
        .contains("Dark")
}

fn truncate(name: &str, width: usize) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= width {
        name.to_string()
    } else {
        let mut s: String = chars[..width.saturating_sub(1)].iter().collect();
        s.push('…');
        s
    }
}
