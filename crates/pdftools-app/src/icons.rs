//! The SVG icons (monochrome; tinted when drawn).

use iced::widget::svg::Handle;

macro_rules! icons {
    ($($name:ident => $file:literal),* $(,)?) => {
        $(pub fn $name() -> Handle {
            Handle::from_memory(include_bytes!(concat!("../icons/", $file, ".svg")).as_slice())
        })*
    };
}

icons! {
    check => "check",
    chevron_left => "chevron-left",
    chevron_right => "chevron-right",
    chevron_up => "chevron-up",
    compress => "compress",
    copy => "copy",
    down => "down",
    external => "external",
    eye => "eye",
    eye_off => "eye-off",
    file => "file",
    folder => "folder",
    github => "github",
    grayscale => "grayscale",
    grip => "grip",
    home => "home",
    image => "image",
    images => "images",
    info => "info",
    merge => "merge",
    monitor => "monitor",
    moon => "moon",
    metadata => "metadata",
    page_numbers => "page-numbers",
    pen => "pen",
    plus => "plus",
    protect => "protect",
    rearrange => "rearrange",
    reset => "reset",
    reverse => "reverse",
    rotate => "rotate",
    rotate_ccw => "rotate-ccw",
    rotate_cw => "rotate-cw",
    select_all => "select-all",
    signature => "signature",
    sort => "sort",
    split => "split",
    sun => "sun",
    trash => "trash",
    type_ => "type",
    unlock => "unlock",
    up => "up",
    upload => "upload",
    watermark => "watermark",
    x => "x",
    zoom => "zoom",
}
