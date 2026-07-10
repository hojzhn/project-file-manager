use iced::widget::{button, column, container, row, text, text_input};
use iced::{Element, Length};

use crate::ui::message::Message;
use crate::ui::state::State;

pub fn view(state: &State) -> Element<'static, Message> {
    let Some(dialog) = &state.scratch_dialog else {
        return column![].into();
    };

    let ready = !dialog.name.trim().is_empty();
    let create_button = if ready {
        button("Create").on_press(Message::ScratchSubmitted)
    } else {
        button("Create")
    };

    let content = column![
        text("New Project from Scratch").size(20),
        row![
            text("Name:"),
            text_input("Project name", &dialog.name).on_input(Message::ScratchNameChanged).width(300.0),
        ]
        .spacing(8.0),
        row![create_button, button("Cancel").on_press(Message::ScratchCancelled)].spacing(8.0),
    ]
    .spacing(12.0)
    .padding(24.0)
    .width(400.0);

    container(content).center(Length::Fill).into()
}
