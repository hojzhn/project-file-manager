use iced::widget::{column, row, text, Space};
use iced::{Element, Length};

use crate::ui::message::Message;
use crate::ui::state::State;
use crate::ui::style;
use crate::ui::view::new_project_menu;
use crate::util::strip_date_prefix;

pub fn view(state: &State) -> Element<'static, Message> {
    let mut project_list = column![].spacing(4.0);
    for project in &state.projects {
        let selected = state.selected_project == Some(project.id);
        let label = strip_date_prefix(&project.name).into_owned();
        project_list = project_list.push(
            style::button(text(label), style::ButtonKind::Text)
                .on_press(Message::ProjectSelected(project.id))
                .width(Length::Fill)
                .style(move |theme, status| {
                     let mut s = iced::widget::button::text  (theme, status);
                      if selected {
                           s.background = Some(iced::Color::from_rgb8(0x3a, 0x3d, 0x45).into());
                     }
                      s
    })
);
    }

    let header = row![
        text("Projects"),
        Space::new().width(Length::Fill),
        style::button("+ New Project", style::ButtonKind::Primary).on_press(Message::NewProjectMenuOpened),
    ]
    .align_y(iced::Alignment::Center);

    let mut col = column![header]
    .spacing(8.0)
    .padding(6.0)
    .width(260.0);

    if state.new_project_menu_open {
        col = col.push(new_project_menu::view(state));
    }

    col = col.push(style::scrollable(project_list).height(Length::Fill));
    col = col.push(style::button("Directories...", style::ButtonKind::Secondary).on_press(Message::OpenSetupDialog));
    col = col.push(style::button("Sync now", style::ButtonKind::Secondary).on_press(Message::SyncNowClicked));

    style::panel(col).height(Length::Fill).into()
}
