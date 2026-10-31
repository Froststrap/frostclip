use iced::alignment::Horizontal;
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length};

use crate::Message;
use crate::draft::{Draft, Key};
use crate::theme;
use crate::widgets::{BOLD, IconFn, Status, filler, icon, lucide, tag};

struct StaticField {
    label: &'static str,
    value: &'static str,
    status: Status,
    advanced: bool,
}

struct StaticToggle {
    label: &'static str,
    on: bool,
    status: Status,
}

const fn field(
    label: &'static str,
    value: &'static str,
    status: Status,
    advanced: bool,
) -> StaticField {
    StaticField {
        label,
        value,
        status,
        advanced,
    }
}

const fn toggle(label: &'static str, on: bool, status: Status) -> StaticToggle {
    StaticToggle { label, on, status }
}

const VIDEO_STATIC: &[StaticField] = &[
    field("Video quality", "—", Status::Todo, false),
    field("Color range", "—", Status::Todo, true),
    field("Video codec", "Auto", Status::InDev, false),
    field("Frame rate mode", "—", Status::Todo, true),
];

const VIDEO_TOGGLES: &[StaticToggle] = &[toggle("Record cursor", true, Status::Todo)];

const FILE_STATIC: &[StaticField] = &[field("Container", "mp4", Status::Ready, false)];

const AUDIO_STATIC: &[StaticField] = &[field("Microphone", "Off", Status::Todo, false)];

const GENERAL_TOGGLES: &[StaticToggle] = &[
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

const NOTIFICATION_TOGGLES: &[StaticToggle] = &[toggle(
    "Show notification when a clip is saved",
    true,
    Status::InDev,
)];

pub fn view<'a>(draft: &Draft, advanced: bool) -> Element<'a, Message> {
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

    let apply = button(
        container(
            row![
                icon(lucide::save, 16.0, theme::BLACK_TEXT),
                text("Apply").size(16)
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .center(Length::Fill),
    )
    .width(Length::Fixed(200.0))
    .height(Length::Fixed(72.0))
    .on_press(Message::Apply)
    .style(theme::segment(true));

    let mut video = column![text("Video").size(15).font(BOLD)].spacing(12);
    video = video.push(static_fields(VIDEO_STATIC.iter().take(1), advanced));
    video = video.push(edit(
        "Video bitrate (kbps)",
        Key::Bitrate,
        Status::Ready,
        draft,
    ));
    if advanced {
        video = video.push(edit("Width", Key::Width, Status::Ready, draft));
        video = video.push(edit("Height", Key::Height, Status::Ready, draft));
    }
    video = video.push(edit("Frame rate", Key::Framerate, Status::Ready, draft));
    video = video.push(static_fields(VIDEO_STATIC.iter().skip(1), advanced));
    video = video.push(toggles(VIDEO_TOGGLES));

    let mut files = column![text("File info").size(15).font(BOLD)].spacing(12);
    files = files.push(edit(
        "Directory to save clips",
        Key::OutputDir,
        Status::Ready,
        draft,
    ));
    files = files.push(static_fields(FILE_STATIC.iter(), advanced));
    files = files.push(edit(
        "Replay time in seconds",
        Key::ReplaySeconds,
        Status::Ready,
        draft,
    ));
    files = files.push(edit(
        "Default clip length in seconds",
        Key::ClipSeconds,
        Status::Ready,
        draft,
    ));
    if advanced {
        files = files.push(readonly(
            "Estimated video max file size in RAM",
            match draft.estimate_megabytes() {
                Some(megabytes) => format!("{megabytes:.1} MB"),
                None => String::from("—"),
            },
            Status::Ready,
        ));
    }

    let mut audio = column![text("Audio").size(15).font(BOLD)].spacing(12);
    audio = audio.push(audio_toggle(draft.audio_enabled()));
    if advanced {
        audio = audio.push(edit(
            "Audio bitrate (kbps)",
            Key::AudioBitrate,
            Status::Ready,
            draft,
        ));
    }
    audio = audio.push(static_fields(AUDIO_STATIC.iter(), advanced));

    let general = column![
        text("General").size(15).font(BOLD),
        toggles(GENERAL_TOGGLES)
    ]
    .spacing(12);
    let notifications = column![
        text("Notifications").size(15).font(BOLD),
        toggles(NOTIFICATION_TOGGLES)
    ]
    .spacing(12);

    let sections = column![
        card(video),
        card(files),
        card(audio),
        card(general),
        card(notifications),
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

    let side = column![back, apply].spacing(2);

    row![left, panel, side]
        .spacing(14)
        .align_y(Alignment::Start)
        .into()
}

fn card<'a>(content: iced::widget::Column<'a, Message>) -> Element<'a, Message> {
    container(content)
        .padding(14)
        .width(Length::Fill)
        .style(|_| theme::section())
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

fn head<'a>(label: &'static str, status: Status) -> Element<'a, Message> {
    row![text(label).size(12).color(theme::MUTED), tag(status)]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
}

fn edit<'a>(label: &'static str, key: Key, status: Status, draft: &Draft) -> Element<'a, Message> {
    let input = text_input("", draft.get(key))
        .on_input(move |value| Message::Edit(key, value))
        .on_submit(Message::Apply)
        .size(13)
        .padding([5, 10])
        .width(Length::Fixed(260.0))
        .style(theme::input);
    column![head(label, status), input].spacing(5).into()
}

fn readonly<'a>(label: &'static str, value: String, status: Status) -> Element<'a, Message> {
    let pill = container(text(value).size(13))
        .padding([5, 10])
        .width(Length::Fixed(260.0))
        .style(|_| theme::pill());
    column![head(label, status), pill].spacing(5).into()
}

fn static_fields<'a>(
    items: impl Iterator<Item = &'static StaticField>,
    advanced: bool,
) -> Element<'a, Message> {
    let mut col = column![].spacing(12);
    for item in items.filter(|item| advanced || !item.advanced) {
        col = col.push(readonly(item.label, item.value.to_string(), item.status));
    }
    col.into()
}

fn toggles<'a>(items: &'static [StaticToggle]) -> Element<'a, Message> {
    let mut col = column![].spacing(12);
    for item in items {
        let (glyph, color): (IconFn<'a>, _) = if item.on {
            (lucide::toggle_right, theme::ACCENT)
        } else {
            (lucide::toggle_left, theme::MUTED)
        };
        col = col.push(
            row![
                icon(glyph, 20.0, color),
                text(item.label).size(13),
                filler(),
                tag(item.status),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        );
    }
    col.into()
}

fn audio_toggle<'a>(on: bool) -> Element<'a, Message> {
    let (glyph, color): (IconFn<'a>, _) = if on {
        (lucide::toggle_right, theme::ACCENT)
    } else {
        (lucide::toggle_left, theme::MUTED)
    };
    button(
        row![
            icon(glyph, 20.0, color),
            text("Desktop audio").size(13),
            filler()
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(0)
    .on_press(Message::ToggleAudio(!on))
    .style(theme::plain)
    .into()
}
