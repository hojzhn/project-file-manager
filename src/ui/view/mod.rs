pub mod new_project_menu;
pub mod project_view;
pub mod scratch_dialog;
pub mod setup_dialog;
pub mod sidebar;
pub mod widgets;

use iced::widget::row;
use iced::Element;

use crate::ui::message::Message;
use crate::ui::state::State;

pub fn view(state: &State) -> Element<'_, Message> {
    if state.setup_dialog.is_some() {
        return setup_dialog::view(state);
    }
    if state.scratch_dialog.is_some() {
        return scratch_dialog::view(state);
    }

    row![sidebar::view(state), project_view::view(state)].into()
}
