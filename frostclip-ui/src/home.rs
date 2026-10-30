use iced::alignment::Vertical;
use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Element, Length};

use crate::theme;
use crate::widgets::{BOLD, IconFn, Status, blank, filler, icon, lucide, tag};
use crate::{Message, Tile};

const TILE_W: f32 = 220.0;
const TILE_H: f32 = 210.0;

pub fn view<'a>(selected: Tile) -> Element<'a, Message> {
    let tiles = row![
        column_for(Tile::Replay, selected),
        column_for(Tile::Record, selected),
        column_for(Tile::Stream, selected),
        gear(),
    ]
    .spacing(2)
    .align_y(Alignment::Start);

    tiles.into()
}

fn column_for<'a>(kind: Tile, selected: Tile) -> Element<'a, Message> {
    let marker: Element<'a, Message> = if kind == selected {
        container(blank())
            .width(Length::Fixed(TILE_W))
            .height(3)
            .style(|_| theme::accent_line())
            .into()
    } else {
        container(blank())
            .width(Length::Fixed(TILE_W))
            .height(3)
            .into()
    };

    let mut col = column![marker, tile(kind, selected)];
    if kind == selected {
        col = col.push(menu(kind));
    }
    col.into()
}

fn tile<'a>(kind: Tile, selected: Tile) -> Element<'a, Message> {
    let (title, glyph, state, status): (&str, IconFn<'a>, &str, Status) = match kind {
        Tile::Replay => (
            "Instant Replay",
            lucide::history,
            "Status unavailable",
            Status::InDev,
        ),
        Tile::Record => ("Record", lucide::circle, "Not recording", Status::Todo),
        Tile::Stream => ("Livestream", lucide::radio, "Not streaming", Status::Todo),
    };

    let content = column![
        text(title).size(16).font(BOLD),
        icon(glyph, 72.0, theme::TEXT),
        text(state).size(13).color(theme::MUTED),
        tag(status),
    ]
    .spacing(14)
    .align_x(Alignment::Center);

    button(container(content).center(Length::Fill))
        .width(Length::Fixed(TILE_W))
        .height(Length::Fixed(TILE_H))
        .padding(10)
        .on_press(Message::Select(kind))
        .style(theme::tile(kind == selected))
        .into()
}

fn gear<'a>() -> Element<'a, Message> {
    let btn = button(icon(lucide::settings, 22.0, theme::TEXT))
        .padding(14)
        .on_press(Message::OpenSettings)
        .style(theme::tile(false));

    container(btn)
        .height(Length::Fixed(TILE_H + 3.0))
        .align_y(Vertical::Bottom)
        .padding([0, 0])
        .into()
}

fn menu<'a>(kind: Tile) -> Element<'a, Message> {
    let items: Vec<(IconFn<'a>, &'static str, Status, Message)> = match kind {
        Tile::Replay => vec![
            (lucide::save, "Save clip", Status::Ready, Message::SaveClip),
            (
                lucide::play,
                "Turn buffer on/off",
                Status::Todo,
                Message::NotReady("Turn buffer on/off"),
            ),
            (
                lucide::settings,
                "Settings",
                Status::Ready,
                Message::OpenSettings,
            ),
        ],
        Tile::Record => vec![
            (
                lucide::play,
                "Start",
                Status::Todo,
                Message::NotReady("Start recording"),
            ),
            (
                lucide::pause,
                "Pause",
                Status::Todo,
                Message::NotReady("Pause recording"),
            ),
            (
                lucide::settings,
                "Settings",
                Status::Ready,
                Message::OpenSettings,
            ),
        ],
        Tile::Stream => vec![
            (
                lucide::play,
                "Start",
                Status::Todo,
                Message::NotReady("Livestream"),
            ),
            (
                lucide::settings,
                "Settings",
                Status::Ready,
                Message::OpenSettings,
            ),
        ],
    };

    let mut col = column![].width(Length::Fixed(TILE_W));
    for (index, (glyph, label, status, message)) in items.into_iter().enumerate() {
        let line = row![
            icon(glyph, 14.0, theme::TEXT),
            text(label).size(14),
            filler(),
            tag(status),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        col = col.push(
            button(line)
                .width(Length::Fill)
                .padding([9, 12])
                .on_press(message)
                .style(theme::menu_item(index == 0)),
        );
    }
    col.into()
}
