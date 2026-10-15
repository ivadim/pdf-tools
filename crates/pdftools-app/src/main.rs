// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::rc::Rc;

use pdftools_app::app::App;
use pdftools_app::dialogs::NativeDialogs;
use pdftools_app::{AppWindow, Tools};
use slint::winit_030::{winit::event::WindowEvent, EventResult, WinitWindowAccessor};
use slint::ComponentHandle;

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let app = App::new(&ui, Rc::new(NativeDialogs));

    // Files dragged in from the file manager.
    let weak_app = Rc::downgrade(&app);
    let weak_ui = ui.as_weak();
    ui.window().on_winit_window_event(move |_, event| {
        let set_dropping = |on: bool| {
            if let Some(ui) = weak_ui.upgrade() {
                ui.global::<Tools>().set_dropping(on);
            }
        };
        match event {
            WindowEvent::HoveredFile(_) => set_dropping(true),
            WindowEvent::HoveredFileCancelled => set_dropping(false),
            WindowEvent::DroppedFile(path) => {
                set_dropping(false);
                if let Some(app) = weak_app.upgrade() {
                    if let Ok(mut app) = app.try_borrow_mut() {
                        app.files_dropped(vec![path.clone()]);
                    }
                    pdftools_app::jobs::drain();
                }
            }
            _ => return EventResult::Propagate,
        }
        EventResult::Propagate
    });

    #[cfg(debug_assertions)]
    dev_snapshot(&ui, &app);

    ui.run()
}

/// Development aid: `PDFTOOLS_SNAPSHOT=out.png PDFTOOLS_OPEN=tool[:file.pdf]`
/// opens a tool (and file), waits for rendering, saves a screenshot and exits.
#[cfg(debug_assertions)]
fn dev_snapshot(ui: &AppWindow, app: &Rc<std::cell::RefCell<App>>) {
    let Some(out) = std::env::var_os("PDFTOOLS_SNAPSHOT") else { return };
    if let Some(open) = std::env::var("PDFTOOLS_OPEN").ok() {
        let (tool, file) = open.split_once(':').map_or((open.as_str(), None), |(t, f)| (t, Some(f)));
        ui.global::<Tools>().set_active(tool.into());
        if let (Some(file), Some(index)) = (file, pdftools_app::tools::id::NAMES.iter().position(|n| *n == tool)) {
            app.borrow_mut().load_file(index, file.into());
        }
    }
    let weak = ui.as_weak();
    let delay = std::env::var("PDFTOOLS_SNAPSHOT_DELAY").ok().and_then(|d| d.parse().ok()).unwrap_or(2500);
    slint::Timer::single_shot(std::time::Duration::from_millis(delay), move || {
        let Some(ui) = weak.upgrade() else { return };
        if let Ok(shot) = ui.window().take_snapshot() {
            let _ = image::save_buffer(&out, shot.as_bytes(), shot.width(), shot.height(), image::ColorType::Rgba8);
        }
        let _ = slint::quit_event_loop();
    });
}
