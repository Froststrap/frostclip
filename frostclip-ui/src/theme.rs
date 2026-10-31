use iced::widget::{button, container, text_input};
use iced::{Background, Border, Color, Theme};

pub const DIM: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.62);
pub const PANEL: Color = Color::from_rgb(0.09, 0.10, 0.09);
pub const PANEL_RAISED: Color = Color::from_rgb(0.13, 0.14, 0.13);
pub const TILE: Color = Color::from_rgb(0.07, 0.07, 0.07);
pub const TILE_HOVER: Color = Color::from_rgb(0.11, 0.12, 0.11);
pub const ACCENT: Color = Color::from_rgb(0.463, 0.725, 0.0);
pub const BLACK_TEXT: Color = Color::BLACK;
pub const TEXT: Color = Color::from_rgb(0.95, 0.95, 0.95);
pub const MUTED: Color = Color::from_rgb(0.62, 0.64, 0.62);
pub const AMBER: Color = Color::from_rgb(0.88, 0.64, 0.10);
pub const BLUE: Color = Color::from_rgb(0.29, 0.62, 0.88);

pub fn bar() -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.78))),
        ..Default::default()
    }
}

pub fn panel() -> container::Style {
    container::Style {
        background: Some(Background::Color(PANEL)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 2.0.into(),
        },
        ..Default::default()
    }
}

pub fn section() -> container::Style {
    container::Style {
        background: Some(Background::Color(PANEL_RAISED)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 2.0.into(),
        },
        ..Default::default()
    }
}

pub fn accent_line() -> container::Style {
    container::Style {
        background: Some(Background::Color(ACCENT)),
        ..Default::default()
    }
}

pub fn status_bar() -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.78))),
        ..Default::default()
    }
}

pub fn tag(color: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color { a: 0.16, ..color })),
        border: Border {
            color,
            width: 1.0,
            radius: 3.0.into(),
        },
        ..Default::default()
    }
}

pub fn pill() -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.05, 0.05, 0.05))),
        border: Border {
            color: Color::from_rgb(0.22, 0.23, 0.22),
            width: 1.0,
            radius: 2.0.into(),
        },
        ..Default::default()
    }
}

pub fn tile(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let background = if selected || hovered {
            TILE_HOVER
        } else {
            TILE
        };
        button::Style {
            background: Some(Background::Color(background)),
            text_color: TEXT,
            border: Border::default(),
            ..Default::default()
        }
    }
}

pub fn menu_item(focused: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let active = focused || hovered;
        button::Style {
            background: Some(Background::Color(if active { TILE_HOVER } else { TILE })),
            text_color: TEXT,
            border: Border {
                color: if active { ACCENT } else { Color::TRANSPARENT },
                width: if active { 2.0 } else { 0.0 },
                radius: 0.0.into(),
            },
            ..Default::default()
        }
    }
}

pub fn segment(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let background = if active {
            ACCENT
        } else if hovered {
            TILE_HOVER
        } else {
            TILE
        };
        button::Style {
            background: Some(Background::Color(background)),
            text_color: if active { Color::BLACK } else { TEXT },
            border: Border::default(),
            ..Default::default()
        }
    }
}

pub fn plain(_: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: if hovered {
            Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.12)))
        } else {
            None
        },
        text_color: TEXT,
        border: Border::default(),
        ..Default::default()
    }
}

pub fn input(_: &Theme, status: text_input::Status) -> text_input::Style {
    let focused = matches!(status, text_input::Status::Focused { .. });
    text_input::Style {
        background: Background::Color(Color::from_rgb(0.05, 0.05, 0.05)),
        border: Border {
            color: if focused {
                ACCENT
            } else {
                Color::from_rgb(0.22, 0.23, 0.22)
            },
            width: 1.0,
            radius: 2.0.into(),
        },
        icon: MUTED,
        placeholder: MUTED,
        value: TEXT,
        selection: Color { a: 0.35, ..ACCENT },
    }
}
