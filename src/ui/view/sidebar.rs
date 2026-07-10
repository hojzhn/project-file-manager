use iced::widget::{button, column, container, scrollable, text};
use iced::{Element, Length};

use crate::ui::message::Message;
use crate::ui::state::State;
use crate::ui::view::new_project_menu;

pub fn view(state: &State) -> Element<'static, Message> {
    let mut project_list = column![].spacing(4.0);
    for project in &state.projects {
        let selected = state.selected_project == Some(project.id);
        let label = if selected { format!("> {}", project.name) } else { format!("  {}", project.name) };
        project_list = project_list.push(
            button(text(label)).on_press(Message::ProjectSelected(project.id)).width(Length::Fill),
        );
    }

    let mut col = column![
        text("Projects").size(20),
        button("+ New Project").on_press(Message::NewProjectMenuOpened),
    ]
    .spacing(8.0)
    .padding(12.0)
    .width(260.0);

    if state.new_project_menu_open {
        col = col.push(new_project_menu::view(state));
    }

    col = col.push(scrollable(project_list).height(Length::Fill));
    col = col.push(button("Directories...").on_press(Message::OpenSetupDialog));
    col = col.push(button("Sync now").on_press(Message::SyncNowClicked));

    container(col).height(Length::Fill).into()
}
