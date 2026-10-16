//! About: version, project links, licence and credits.

use iced::widget::{button, column, container, image, row, scrollable, text};
use iced::{Alignment, Element, Length, Padding};

use crate::app::Message;
use crate::icons;
use crate::theme;
use crate::widgets::{action, icon, muted, section_title, BOLD};

pub const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// The system and processor, as people know them: "macOS arm64".
pub fn platform() -> String {
    let os = match std::env::consts::OS {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        other => other,
    };
    format!("{os} {arch}")
}

fn link<'a>(label: &'a str, url: String) -> Element<'a, Message> {
    button(row![text(label), icon(icons::external(), 13.0, theme::ACCENT)].spacing(4).align_y(Alignment::Center))
        .padding(0)
        .style(theme::link)
        .on_press(Message::OpenUrl(url))
        .into()
}

fn panel<'a>(title: &str, content: impl IntoIterator<Item = Element<'a, Message>>) -> Element<'a, Message> {
    container(column![section_title(title)].extend(content).spacing(10))
        .width(Length::Fill)
        .padding(18)
        .style(theme::panel)
        .into()
}

fn credit<'a>(name: &'a str, purpose: &'a str, url: &str) -> Element<'a, Message> {
    row![container(link(name, url.into())).width(140), muted(purpose)].align_y(Alignment::Center).into()
}

pub fn view<'a>() -> Element<'a, Message> {
    let logo = image::Handle::from_bytes(include_bytes!("../../../packaging/icon-256.png").as_slice());
    let content = column![
        row![
            image(logo).width(96).height(96),
            column![
                text("PDF Tools").size(28).font(BOLD),
                text(format!("Version {} · {}", env!("CARGO_PKG_VERSION"), platform())).color(theme::MUTED),
                text("Merge, split, protect and edit PDFs — on your computer."),
            ]
            .spacing(4),
        ]
        .spacing(20)
        .align_y(Alignment::Center),
        row![
            action("GitHub", Some(icons::github()), true, Some(Message::OpenUrl(REPOSITORY.into()))),
            action("Releases and updates", Some(icons::external()), false, Some(Message::OpenUrl(format!("{REPOSITORY}/releases")))),
            action("Report a problem", Some(icons::external()), false, Some(Message::OpenUrl(format!("{REPOSITORY}/issues/new")))),
        ]
        .spacing(8),
        panel(
            "Privacy",
            [text("Everything runs on your computer. PDF Tools never uploads your files, and it doesn't connect to the internet unless you open one of the links on this page.").into()],
        ),
        panel(
            "Licence",
            [
                text("PDF Tools is free and open-source software, released under the MIT licence. Copyright © 2026 Dmitry Ivanov.").into(),
                row![
                    link("MIT licence", format!("{REPOSITORY}/blob/main/LICENSE")),
                    link("Source code", REPOSITORY.into()),
                ]
                .spacing(18)
                .into(),
            ],
        ),
        panel(
            "Built with",
            [
                muted("PDF Tools stands on these open-source projects, among others:"),
                credit("iced", "User interface toolkit", "https://iced.rs"),
                credit("lopdf", "Reading, editing and writing PDF files", "https://crates.io/crates/lopdf"),
                credit("hayro", "Rendering page previews", "https://crates.io/crates/hayro"),
                credit("image", "Reading and writing JPEG and PNG images", "https://crates.io/crates/image"),
                credit("Parley", "Text layout for watermarks and signatures", "https://crates.io/crates/parley"),
                credit("tiny-skia", "Drawing, and rendering the interface", "https://crates.io/crates/tiny-skia"),
                credit("rfd", "Native file dialogs", "https://crates.io/crates/rfd"),
            ],
        ),
    ]
    .spacing(18)
    .padding(Padding { top: 36.0, right: 28.0, bottom: 36.0, left: 28.0 });
    scrollable(content).into()
}
