use iced::widget::{column, text};
use iced::Element;

use crate::ui::message::Message;
use crate::ui::state::State;
use crate::ui::style;

pub fn view(_state: &State) -> Element<'static, Message> {
    column![
        style::button("Select image from Downloads...", style::ButtonKind::Primary)
            .on_press(Message::PickImageFromDownloads),
        style::button("Browse for image...", style::ButtonKind::Primary).on_press(Message::BrowseForImage),
        style::button("Create from scratch...", style::ButtonKind::Primary).on_press(Message::StartScratchProject),
        style::button("Close", style::ButtonKind::Secondary).on_press(Message::NewProjectMenuClosed),
        text("...or drag and drop an image onto this window."),
    ]
    .spacing(4.0)
    .into()
}
