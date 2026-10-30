use iced::alignment::Horizontal;
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Length};

use crate::Message;
use crate::theme;
use crate::widgets::{BOLD, IconFn, Status, filler, icon, lucide, tag};

struct Field {
    label: &'static str,
    value: &'static str,
    status: Status,
    advanced: bool,
}

struct Toggle {
    label: &'static str,
    on: bool,
    status: Status,
}

const fn field(label: &'static str, value: &'static str, status: Status, advanced: bool) -> Field {
    Field {
        label,
        value,
        status,
        advanced,
    }
}

const fn toggle(label: &'static str, on: bool, status: Status) -> Toggle {
    Toggle { label, on, status }
}

const VIDEO: &[Field] = &[
    field("Video quality", "—", Status::Todo, false),
    field("Video bitrate (kbps)", "—", Status::Todo, true),
    field("Color range", "—", Status::Todo, true),
    field("Video codec", "Auto", Status::InDev, false),
    field("Frame rate", "Auto", Status::InDev, false),
    field("Frame rate mode", "—", Status::Todo, true),
];

const VIDEO_TOGGLES: &[Toggle] = &[toggle("Record cursor", true, Status::Todo)];

const FILE_INFO: &[Field] = &[
    field("Directory to save clips", "—", Status::InDev, false),
    field("Container", "mp4", Status::Ready, false),
    field("Replay time in seconds", "—", Status::InDev, false),
    field(
        "Estimated video max file size in RAM",
        "—",
        Status::Todo,
        true,
    ),
];

const AUDIO: &[Field] = &[
    field("Desktop audio", "On", Status::InDev, false),
    field("Audio bitrate (kbps)", "192", Status::InDev, true),
    field("Microphone", "Off", Status::Todo, false),
];

const GENERAL_TOGGLES: &[Toggle] = &[
    toggle("Start daemon on login", false, Status::Todo),
    toggle(
        "Turn on replay when starting a fullscreen application",
        false,
        Status::Todo,
    ),
    toggle(
        "Turn on replay when power supply is connected",
        false,
        Status::Todo,
    ),
    toggle(
        "Save video in a folder with the name of the game",
        false,
        Status::Todo,
    ),
];

const NOTIFICATION_TOGGLES: &[Toggle] = &[toggle(
    "Show notification when a clip is saved",
    true,
    Status::InDev,
)];

pub fn view<'a>(advanced: bool) -> Element<'a, Message> {
    let left = container(
        column![
            text("Settings").size(16).font(BOLD),
            icon(lucide::settings, 72.0, theme::TEXT),
        ]
        .spacing(18)
        .align_x(Alignment::Center),
    )
    .center(Length::Fixed(220.0))
    .height(Length::Fixed(210.0))
    .style(|_| theme::bar());

    let back = button(
        container(
            row![
                icon(lucide::arrow_left, 16.0, theme::TEXT),
                text("Back").size(16)
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .center(Length::Fill),
    )
    .width(Length::Fixed(200.0))
    .height(Length::Fixed(72.0))
    .on_press(Message::Back)
    .style(theme::tile(false));

    let sections = column![
        section("Video", VIDEO, VIDEO_TOGGLES, advanced),
        section("File info", FILE_INFO, &[], advanced),
        section("Audio", AUDIO, &[], advanced),
        section("General", &[], GENERAL_TOGGLES, advanced),
        section("Notifications", &[], NOTIFICATION_TOGGLES, advanced),
    ]
    .spacing(10)
    .padding([0, 12]);

    let panel = container(
        column![
            container(view_switch(advanced))
                .width(Length::Fill)
                .align_x(Horizontal::Center),
            scrollable(sections).height(Length::Fill),
        ]
        .spacing(10)
        .padding(10),
    )
    .width(Length::Fixed(620.0))
    .height(Length::Fixed(540.0))
    .style(|_| theme::panel());

    row![left, panel, back]
        .spacing(14)
        .align_y(Alignment::Start)
        .into()
}

fn view_switch<'a>(advanced: bool) -> Element<'a, Message> {
    let simple = button(text("Simple view").size(12))
        .padding([4, 10])
        .on_press(Message::SetAdvanced(false))
        .style(theme::segment(!advanced));
    let adv = button(text("Advanced view").size(12))
        .padding([4, 10])
        .on_press(Message::SetAdvanced(true))
        .style(theme::segment(advanced));
    row![simple, adv].into()
}

fn section<'a>(
    title: &'static str,
    fields: &'static [Field],
    toggles: &'static [Toggle],
    advanced: bool,
) -> Element<'a, Message> {
    let mut col = column![text(title).size(15).font(BOLD)].spacing(12);

    for item in fields.iter().filter(|f| advanced || !f.advanced) {
        col = col.push(field_view(item));
    }
    for item in toggles {
        col = col.push(toggle_view(item));
    }

    container(col)
        .padding(14)
        .width(Length::Fill)
        .style(|_| theme::section())
        .into()
}

fn field_view<'a>(item: &'static Field) -> Element<'a, Message> {
    let head = row![
        text(item.label).size(12).color(theme::MUTED),
        tag(item.status)
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let value = container(text(item.value).size(13))
        .padding([5, 10])
        .width(Length::Fixed(260.0))
        .style(|_| theme::pill());
    column![head, value].spacing(5).into()
}

fn toggle_view<'a>(item: &'static Toggle) -> Element<'a, Message> {
    let (glyph, color): (IconFn<'a>, _) = if item.on {
        (lucide::toggle_right, theme::ACCENT)
    } else {
        (lucide::toggle_left, theme::MUTED)
    };
    row![
        icon(glyph, 20.0, color),
        text(item.label).size(13),
        filler(),
        tag(item.status),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}
