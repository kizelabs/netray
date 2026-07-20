use tray_icon::Icon;

const SIZE: usize = 22;

pub fn blank_icon() -> Icon {
    let img = vec![0u8; SIZE * SIZE * 4];
    Icon::from_rgba(img, SIZE as u32, SIZE as u32).expect("icon")
}