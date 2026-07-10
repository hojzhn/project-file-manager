use iced::widget::{button, column, container, row, text, text_input};
use iced::{Element, Length};

use crate::ui::message::Message;
use crate::ui::state::State;

pub fn view(state: &State) -> Element<'static, Message> {
    let Some(dialog) = &state.setup_dialog else {
        return column![].into();
    };

    let root_label = dialog.root.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "Not set".to_string());

    let mut dirs_col = column![text("Relevant directories (e.g. RIP output folders):")].spacing(4.0);
    for (index, dir) in dialog.relevant_dirs.iter().enumerate() {
        dirs_col = dirs_col.push(
            row![
                button("Remove").on_press(Message::SetupRelevantDirRemoved(index)),
                text(dir.display().to_string()),
            ]
            .spacing(8.0),
        );
    }

    let ready = dialog.root.is_some() && !dialog.relevant_dirs.is_empty();
    let continue_button =
        if ready { button("Continue").on_press(Message::SetupSubmitted) } else { button("Continue") };

    let content = column![
        text("Set up directories").size(20),
        text(
            "Choose where project folders are created, and which folder(s) hold output from \
             external tools (like PrintFactory RIP) that should be matched into projects by \
             filename."
        ),
        row![button("Choose root directory...").on_press(Message::SetupRootPicked), text(root_label)].spacing(8.0),
        dirs_col,
        button("+ Add folder...").on_press(Message::SetupRelevantDirAdded),
        text("Source (\"parent\") file extensions, comma-separated:"),
        text_input("png, jpg, jpeg, bmp", &dialog.parent_extensions)
            .on_input(Message::SetupParentExtensionsChanged)
            .width(400.0),
        text("Iteration (\"child\") output extensions, comma-separated:"),
        text_input("prt, bmp", &dialog.child_extensions)
            .on_input(Message::SetupChildExtensionsChanged)
            .width(400.0),
        text(
            "An extension in both lists (like the default bmp) is treated as a source file \
             unless a sibling with a child-only extension (like prt) shares its name."
        )
        .size(12),
        continue_button,
    ]
    .spacing(12.0)
    .padding(24.0)
    .width(520.0);

    container(content).center(Length::Fill).into()
}
