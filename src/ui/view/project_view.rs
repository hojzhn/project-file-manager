use std::collections::BTreeSet;

use chrono::NaiveDate;
use iced::widget::{column, container, row, text};
use iced::{Element, Length};

use crate::grouping::group_files;
use crate::model::{FileSource, ProjectFileView};
use crate::ui::message::Message;
use crate::ui::state::State;
use crate::ui::style;
use crate::ui::view::widgets::{file_row, image_group_view, iteration_view};
use crate::util::strip_date_prefix;
pub fn view(state: &State) -> Element<'static, Message> {
    let mut col = column![].spacing(8.0).padding(12.0).width(Length::Fill);

    if let Some(status) = &state.status {
        col = col.push(
            row![text(status.clone()), style::button("dismiss", style::ButtonKind::Text).on_press(Message::StatusDismissed)]
                .spacing(8.0),
        );
    }

    let Some(project_id) = state.selected_project else {
        col = col.push(text("Select a project, or create a new one."));
        col = col.push(text("Tip: drag and drop an image onto this window to start a project from it."));
        return container(col).into();
    };
    let Some(project) = state.projects.iter().find(|p| p.id == project_id) else {
        return container(col).into();
    };

    col = col.push(
        row![
            text(strip_date_prefix(&project.name).into_owned()),
            style::button("Rescan", style::ButtonKind::Secondary).on_press(Message::RescanProjectClicked),
            style::button("Sync relevant directories", style::ButtonKind::Secondary)
                .on_press(Message::SyncRelevantDirectoriesClicked),
        ]
        .spacing(8.0),
    );
    col = col.push(text(format!("Folder: {}", project.folder_path.display())));

    let files = state.files_by_project.get(&project_id).cloned().unwrap_or_default();
    let pending_child_files = files.iter().any(|f| f.source == FileSource::Child);
    if pending_child_files {
        col = col.push(
            style::button("Move all matched files into project", style::ButtonKind::Primary)
                .on_press(Message::MoveAllMatchedClicked),
        );
    }

    col = col.push(active_dates_row(&files));

    if files.is_empty() {
        col = col.push(text("No files yet."));
        return container(col).into();
    }

    let (image_groups, unassigned_iterations, leftovers) = group_files(&files, &state.settings.extension_rules);

    if image_groups.is_empty() {
        col = col.push(text("No images in this project yet."));
    }
    for group in &image_groups {
        col = col.push(image_group_view(state, group));
    }

    if !unassigned_iterations.is_empty() {
        col = col.push(text(format!("Unmatched iterations ({})", unassigned_iterations.len())));
        let mut iter_col = column![].spacing(4.0);
        for iteration in &unassigned_iterations {
            iter_col = iter_col.push(iteration_view(state, iteration));
        }
        col = col.push(style::scrollable(iter_col).height(400.0));
    }

    if !leftovers.is_empty() {
        col = col.push(text(format!("Other files ({})", leftovers.len())));
        let mut leftover_col = column![].spacing(4.0);
        for file in &leftovers {
            leftover_col = leftover_col.push(file_row(file));
        }
        col = col.push(style::scrollable(leftover_col));
    }

    container(style::scrollable(col)).into()
}

fn active_dates_row(files: &[ProjectFileView]) -> Element<'static, Message> {
    let mut dates: BTreeSet<NaiveDate> = BTreeSet::new();
    for f in files {
        dates.insert(f.created_at.date_naive());
        dates.insert(f.modified_at.date_naive());
    }
    if dates.is_empty() {
        return column![].into();
    }

    let mut chips = row![text("Active dates:")].spacing(6.0);
    for date in dates {
        chips = chips.push(style::panel(text(date.format("%Y-%m-%d").to_string())).padding(4.0));
    }
    chips.into()
}
