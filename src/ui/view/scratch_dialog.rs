use iced::widget::{column, row, text};
use iced::{Element, Length};

use crate::ui::message::Message;
use crate::ui::state::State;
use crate::ui::style;

pub fn view(state: &State) -> Element<'static, Message> {
    let Some(dialog) = &state.scratch_dialog else {
        return column![].into();
    };

    let ready = !dialog.name.trim().is_empty();
    let create_button = if ready {
        style::button("Create", style::ButtonKind::Primary).on_press(Message::ScratchSubmitted)
    } else {
        style::button("Create", style::ButtonKind::Primary)
    };

    let content = column![
        text("New Project from Scratch"),
        row![
            text("Name:"),
            style::text_input("Project name", &dialog.name).on_input(Message::ScratchNameChanged).width(300.0),
        ]
        .spacing(8.0),
        row![
            create_button,
            style::button("Cancel", style::ButtonKind::Secondary).on_press(Message::ScratchCancelled)
        ]
        .spacing(8.0),
    ]
    .spacing(12.0)
    .padding(24.0)
    .width(400.0);

    style::panel(content).center(Length::Fill).into()
}
