use iced::widget::{Text, column, container, text};
use iced::{Color, Element, Font, Length};
pub use iced_fonts::lucide;

use crate::Message;
use crate::theme;

pub const BOLD: Font = Font {
    weight: iced::font::Weight::Bold,
    ..Font::DEFAULT
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ready,
    Todo,
    InDev,
}

pub type IconFn<'a> = fn() -> Text<'a>;

pub fn icon<'a>(glyph: IconFn<'a>, size: f32, color: Color) -> Text<'a> {
    glyph().size(size).color(color)
}

pub fn blank<'a>() -> Element<'a, Message> {
    column![].into()
}

pub fn filler<'a>() -> Element<'a, Message> {
    container(blank()).width(Length::Fill).into()
}

pub fn tag<'a>(status: Status) -> Element<'a, Message> {
    let (label, color) = match status {
        Status::Ready => return blank(),
        Status::Todo => ("TODO", theme::AMBER),
        Status::InDev => ("IN DEVELOPMENT", theme::BLUE),
    };
    container(text(label).size(10).font(BOLD).color(color))
        .padding([2, 6])
        .style(move |_| theme::tag(color))
        .into()
}
