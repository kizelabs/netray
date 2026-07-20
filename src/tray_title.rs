#![cfg(target_os = "macos")]

use objc2::{msg_send, msg_send_id};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, ProtocolObject};
use objc2::ClassType;
use objc2_app_kit::{NSMutableParagraphStyle, NSBaselineOffsetAttributeName, NSButton, NSFont, NSFontAttributeName, NSLeftTextAlignment, NSParagraphStyleAttributeName};
use objc2_foundation::{NSAttributedString, NSAttributedStringKey, NSDictionary, NSMutableDictionary, NSNumber, NSString};
use std::sync::OnceLock;

const FONT_SIZE: f64 = 9.0;
/// Fixed height of each line box. The system font's natural line height at 9pt is
/// ~11pt, which stacks to a gap far too wide for the 22pt menu bar. Pinning min ==
/// max collapses each line to exactly this height.
const LINE_HEIGHT: f64 = 9.5;
/// Shifts the whole two-line block up/down inside the button. Positive = up.
/// Pinning the line height leaves the block sitting high in the bar, since the
/// button centers on the font's natural metrics rather than the collapsed box.
/// Measured against a 30pt (notched) menu bar; nudge if yours differs.
const BASELINE_OFFSET: f64 = -4.0;

static STATUS_ITEM_BUTTON: OnceLock<usize> = OnceLock::new();

pub fn set_two_line_title(line1: &str, line2: &str) {
    let button_ptr = match STATUS_ITEM_BUTTON.get() {
        Some(p) => *p,
        None => {
            let p = unsafe { find_status_item_button() };
            if p == 0 {
                return;
            }
            let _ = STATUS_ITEM_BUTTON.set(p);
            p
        }
    };
    if button_ptr == 0 {
        return;
    }
    unsafe {
        apply_two_line_title(button_ptr as *mut AnyObject, &format!("{}\n{}", line1, line2));
    }
}

unsafe fn class_name(obj: *mut AnyObject) -> String {
    if obj.is_null() {
        return String::new();
    }
    let cls: *const AnyClass = msg_send![obj, class];
    if cls.is_null() {
        return String::new();
    }
    (*cls).name().to_string()
}

unsafe fn find_status_item_button() -> usize {
    let app: *mut AnyObject = msg_send![objc2::class!(NSApplication), sharedApplication];
    if app.is_null() {
        return 0;
    }
    let windows: *mut AnyObject = msg_send![app, windows];
    if windows.is_null() {
        return 0;
    }
    let count: usize = msg_send![windows, count];
    for i in 0..count {
        let window: *mut AnyObject = msg_send![windows, objectAtIndex: i];
        if window.is_null() {
            continue;
        }
        let level: i64 = msg_send![window, level];
        if level != 25 {
            continue;
        }
        let content_view: *mut AnyObject = msg_send![window, contentView];
        if let Some(btn) = find_button_in_view(content_view, 0) {
            return btn;
        }
    }
    0
}

unsafe fn find_button_in_view(view: *mut AnyObject, depth: usize) -> Option<usize> {
    if view.is_null() || depth > 6 {
        return None;
    }
    let name = class_name(view);
    if name == "NSStatusBarButton" {
        return Some(view as usize);
    }
    let subviews: *mut AnyObject = msg_send![view, subviews];
    if subviews.is_null() {
        return None;
    }
    let count: usize = msg_send![subviews, count];
    for i in 0..count {
        let child: *mut AnyObject = msg_send![subviews, objectAtIndex: i];
        if let Some(btn) = find_button_in_view(child, depth + 1) {
            return Some(btn);
        }
    }
    None
}

unsafe fn apply_two_line_title(button: *mut AnyObject, text: &str) {
    let text_ns = NSString::from_str(text);
    // Monospaced, not the proportional system font: with left alignment the block
    // is only width-stable if every glyph is the same width, so the padded field
    // in main.rs actually renders to a constant pixel width.
    // objc2-app-kit 0.2 has no binding for this, hence the raw send.
    // NSFontWeightRegular == 0.0.
    let font: Option<Retained<NSFont>> = msg_send_id![
        NSFont::class(),
        monospacedSystemFontOfSize: FONT_SIZE,
        weight: 0.0f64
    ];
    let font = font.unwrap_or_else(|| NSFont::systemFontOfSize(FONT_SIZE));

    let paragraph_style = NSMutableParagraphStyle::init(NSMutableParagraphStyle::alloc());
    paragraph_style.setAlignment(NSLeftTextAlignment);
    paragraph_style.setLineSpacing(0.0);
    // "\n" makes these two separate paragraphs, so paragraphSpacing — not
    // lineSpacing — is what was opening up the gap. Both must be zero.
    paragraph_style.setParagraphSpacing(0.0);
    paragraph_style.setParagraphSpacingBefore(0.0);
    paragraph_style.setMinimumLineHeight(LINE_HEIGHT);
    paragraph_style.setMaximumLineHeight(LINE_HEIGHT);

    let offset = NSNumber::new_f64(BASELINE_OFFSET);

    let mut dict: Retained<NSMutableDictionary<NSAttributedStringKey, AnyObject>> =
        NSMutableDictionary::dictionaryWithCapacity(3);
    dict.setObject_forKey(&*font, ProtocolObject::from_ref(NSFontAttributeName));
    dict.setObject_forKey(
        &*paragraph_style,
        ProtocolObject::from_ref(NSParagraphStyleAttributeName),
    );
    dict.setObject_forKey(
        &*offset,
        ProtocolObject::from_ref(NSBaselineOffsetAttributeName),
    );

    let attrs: &NSDictionary<NSAttributedStringKey, AnyObject> =
        &*(dict.as_ref() as *const NSMutableDictionary<NSAttributedStringKey, AnyObject>
            as *const NSDictionary<NSAttributedStringKey, AnyObject>);

    let attr = NSAttributedString::initWithString_attributes(
        NSAttributedString::alloc(),
        &text_ns,
        Some(attrs),
    );

    let button_ref = &*(button as *const NSButton);
    button_ref.setAttributedTitle(&attr);

    // No icon, so ensure only the title is measured/laid out
    let _: () = msg_send![button, setImagePosition: 0usize];
    let _: () = msg_send![button, setImage: 0usize as *mut AnyObject];
    // Shrink the bezel/intrinsic padding by disabling the border
    let _: () = msg_send![button, setBezelStyle: 0i64];
}
