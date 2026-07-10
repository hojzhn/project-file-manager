use std::time::Duration;

use iced::Subscription;

use crate::ui::message::Message;
use crate::ui::state::State;

pub fn subscription(_state: &State) -> Subscription<Message> {
    iced::time::every(Duration::from_millis(50)).map(|_| Message::Tick)
}
