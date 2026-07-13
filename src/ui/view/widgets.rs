use iced::widget::{column, image, row, text};
use iced::{Alignment, Element, Length};

use crate::grouping::{ImageGroup, Iteration};
use crate::model::{FileSource, ProjectFileView};
use crate::ui::message::Message;
use crate::ui::state::State;
use crate::ui::style;

pub fn file_row(file: &ProjectFileView) -> Element<'static, Message> {
    let mut r = row![
        text(file.file_name.clone()),
        text(format!("{} bytes", file.size_bytes)),
        text(file.modified_at.format("%Y-%m-%d").to_string()),
        style::button("Open", style::ButtonKind::Secondary).on_press(Message::OpenFileClicked(file.abs_path.clone())),
        style::button("Reveal", style::ButtonKind::Secondary)
            .on_press(Message::RevealFileClicked(file.abs_path.clone())),
    ]
    .spacing(8.0)
    .align_y(Alignment::Center);

    r = match file.source {
        FileSource::Home => r.push(text("home")),
        FileSource::Child => {
            r = r.push(text("not moved"));
            match file.child_file_id {
                Some(child_file_id) => r.push(
                    style::button("Move", style::ButtonKind::Primary)
                        .on_press(Message::MoveChildFileClicked(child_file_id)),
                ),
                None => r,
            }
        }
    };

    r.into()
}

fn thumbnail_or_placeholder(state: &State, file: Option<&ProjectFileView>, size: f32, placeholder: &'static str) -> Element<'static, Message> {
    match file.and_then(|f| state.thumbnail(f)) {
        Some(handle) => image(handle).width(size).height(size).into(),
        None => text(placeholder).into(),
    }
}

pub fn iteration_view(state: &State, iteration: &Iteration) -> Element<'static, Message> {
    let thumb_file = iteration.files.iter().find(|f| state.settings.extension_rules.is_parent(&f.ext));
    let thumb = thumbnail_or_placeholder(state, thumb_file, 96.0, "(no thumbnail yet)");

    let mut files_col = column![text(iteration.label.clone())].spacing(4.0);
    for file in &iteration.files {
        files_col = files_col.push(file_row(file));
    }

    style::panel(row![thumb, files_col].spacing(8.0)).width(Length::Fill).into()
}

pub fn image_group_view(state: &State, group: &ImageGroup) -> Element<'static, Message> {
    let thumb = thumbnail_or_placeholder(state, Some(&group.image), 128.0, "(loading...)");

    let info = column![
        text(group.image.file_name.clone()),
        text(group.image.modified_at.format("%Y-%m-%d").to_string()),
        style::button("Open", style::ButtonKind::Primary)
            .on_press(Message::OpenFileClicked(group.image.abs_path.clone())),
    ]
    .spacing(4.0);

    let mut col = column![row![thumb, info].spacing(8.0)].spacing(8.0);

    if !group.iterations.is_empty() {
        col = col.push(text(format!("Iterations ({})", group.iterations.len())));
        for iteration in &group.iterations {
            col = col.push(iteration_view(state, iteration));
        }
    }

    style::panel(col).width(Length::Fill).into()
}
