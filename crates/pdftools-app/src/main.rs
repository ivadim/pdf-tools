// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> iced::Result {
    pdftools_app::app::run()
}
