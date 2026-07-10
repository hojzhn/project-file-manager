use iced::widget::{button, column, text};
use iced::Element;

use crate::ui::message::Message;
use crate::ui::state::State;

pub fn view(_state: &State) -> Element<'static, Message> {
    column![
        button("Select image from Downloads...").on_press(Message::PickImageFromDownloads),
        button("Browse for image...").on_press(Message::BrowseForImage),
        button("Create from scratch...").on_press(Message::StartScratchProject),
        button("Close").on_press(Message::NewProjectMenuClosed),
        text("...or drag and drop an image onto this window.").size(12),
    ]
    .spacing(4.0)
    .into()
}
