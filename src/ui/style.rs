use iced::theme::Palette;
use iced::widget::{Button, Container, Scrollable, TextInput};
use iced::{border, Border, Color, Element, Theme};

pub const RADIUS: f32 = 0.0;
pub const DEFAULT_TEXT_SIZE: f32 = 11.0;
const PANEL_BORDER_WIDTH: f32 = 1.0;
const BUTTON_PADDING: [f32; 2] = [2.0, 4.0];


pub fn theme() -> Theme {
    Theme::custom(
        "Matr".to_string(),
        Palette {
            background: Color::from_rgb8(0x1c, 0x1d, 0x21),
            text: Color::from_rgb8(0xe8, 0xe8, 0xec),
            primary: Color::from_rgb8(0x6c, 0x8d, 0xf5),
            success: Color::from_rgb8(0x4c, 0xaf, 0x50),
            warning: Color::from_rgb8(0xe0, 0xa5, 0x3e),
            danger: Color::from_rgb8(0xe5, 0x53, 0x53),
        },
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Danger,
    Text,
}

fn button_style(kind: ButtonKind, theme: &Theme, status: iced::widget::button::Status) -> iced::widget::button::Style {
    let mut style = match kind {
        ButtonKind::Primary => iced::widget::button::primary(theme, status),
        ButtonKind::Secondary => iced::widget::button::secondary(theme, status),
        ButtonKind::Danger => iced::widget::button::danger(theme, status),
        ButtonKind::Text => iced::widget::button::text(theme, status),
    };
    style.border = border::rounded(RADIUS);
    style
}

fn panel_style(theme: &Theme) -> iced::widget::container::Style {
    let palette = theme.extended_palette();
    iced::widget::container::rounded_box(theme).border(
        Border::default().rounded(RADIUS).width(PANEL_BORDER_WIDTH).color(palette.background.strong.color),
    )
}

fn text_input_style(theme: &Theme, status: iced::widget::text_input::Status) -> iced::widget::text_input::Style {
    let mut style = iced::widget::text_input::default(theme, status);
    style.border = style.border.rounded(RADIUS);
    style
}

fn scrollable_style(theme: &Theme, status: iced::widget::scrollable::Status) -> iced::widget::scrollable::Style {
    let mut style = iced::widget::scrollable::default(theme, status);
    style.vertical_rail.border = style.vertical_rail.border.rounded(RADIUS);
    style.horizontal_rail.border = style.horizontal_rail.border.rounded(RADIUS);
    style
}

pub fn button<'a, Message: 'a>(content: impl Into<Element<'a, Message>>, kind: ButtonKind) -> Button<'a, Message> {
    iced::widget::button(content).style(move |theme, status| button_style(kind, theme, status)).padding(BUTTON_PADDING)
}

pub fn panel<'a, Message: 'a>(content: impl Into<Element<'a, Message>>) -> Container<'a, Message> {
    iced::widget::container(content).style(panel_style).padding(8.0)
}

pub fn text_input<'a, Message: 'a + Clone>(placeholder: &str, value: &str) -> TextInput<'a, Message> {
    iced::widget::text_input(placeholder, value).style(text_input_style)
}

pub fn scrollable<'a, Message: 'a>(content: impl Into<Element<'a, Message>>) -> Scrollable<'a, Message> {
    iced::widget::scrollable(content).style(scrollable_style)
}
