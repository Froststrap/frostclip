mod home;
mod settings;
mod theme;
mod widgets;

use iced::alignment::Horizontal;
use iced::keyboard::{self, key::Named, Key};
use iced::widget::{button, column, container, row, text};
use iced::{event, Alignment, Element, Event, Length, Subscription, Task, Theme};
use iced_fonts::LUCIDE_FONT_BYTES;
use iced_layershell::application;
use iced_layershell::reexport::{Anchor, KeyboardInteractivity, Layer};
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};
use iced_layershell::to_layer_message;

use widgets::{icon, lucide, BOLD};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tile {
    Replay,
    Record,
    Stream,
}

struct App {
    page: Page,
    selected: Tile,
    advanced: bool,
    notice: String,
}

impl Default for App {
    fn default() -> Self {
        Self {
            page: Page::Home,
            selected: Tile::Replay,
            advanced: false,
            notice: String::new(),
        }
    }
}

#[to_layer_message]
#[derive(Debug, Clone)]
pub enum Message {
    Select(Tile),
    OpenSettings,
    Back,
    Close,
    SaveClip,
    NotReady(&'static str),
    SetAdvanced(bool),
    Event(Event),
}

fn main() -> Result<(), iced_layershell::Error> {
    application(App::default, namespace, update, view)
        .style(style)
        .subscription(subscription)
        .settings(Settings {
            fonts: vec![LUCIDE_FONT_BYTES.into()],
            layer_settings: LayerShellSettings {
                size: None,
                exclusive_zone: -1,
                anchor: Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right,
                layer: Layer::Overlay,
                keyboard_interactivity: KeyboardInteractivity::Exclusive,
                start_mode: StartMode::Active,
                ..Default::default()
            },
            ..Default::default()
        })
        .run()
}

fn namespace() -> String {
    String::from("frostclip")
}

fn subscription(_: &App) -> Subscription<Message> {
    event::listen().map(Message::Event)
}

fn style(_: &App, _: &Theme) -> iced::theme::Style {
    iced::theme::Style {
        background_color: theme::DIM,
        text_color: theme::TEXT,
    }
}

fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::Select(tile) => {
            app.selected = tile;
            Task::none()
        }
        Message::OpenSettings => {
            app.page = Page::Settings;
            Task::none()
        }
        Message::Back => {
            app.page = Page::Home;
            Task::none()
        }
        Message::Close => iced::exit(),
        Message::SaveClip => {
            std::thread::spawn(|| {
                let _ = std::process::Command::new("frostclip").arg("save").status();
            });
            app.notice = String::from("Save requested");
            Task::none()
        }
        Message::NotReady(name) => {
            app.notice = format!("{name}: not implemented yet");
            Task::none()
        }
        Message::SetAdvanced(value) => {
            app.advanced = value;
            Task::none()
        }
        Message::Event(Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Named(Named::Escape),
            ..
        })) => {
            if app.page == Page::Settings {
                app.page = Page::Home;
                Task::none()
            } else {
                iced::exit()
            }
        }
        _ => Task::none(),
    }
}

fn view(app: &App) -> Element<'_, Message> {
    let body = match app.page {
        Page::Home => home::view(app.selected),
        Page::Settings => settings::view(app.advanced),
    };

    let top = container(
        row![
            container(icon(lucide::video, 20.0, theme::ACCENT)).padding([8, 14]),
            container(text("FrostClip").size(17).font(BOLD))
                .width(Length::Fill)
                .align_x(Horizontal::Center),
            button(icon(lucide::x, 18.0, theme::TEXT))
                .padding([8, 14])
                .on_press(Message::Close)
                .style(theme::plain),
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .style(|_| theme::bar());

    let hint = if app.notice.is_empty() {
        String::from("Esc to close")
    } else {
        app.notice.clone()
    };

    let bottom = container(text(hint).size(13).color(theme::MUTED))
        .padding([6, 14])
        .width(Length::Fill)
        .style(|_| theme::status_bar());

    column![top, container(body).center(Length::Fill), bottom,]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
