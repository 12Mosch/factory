use bevy::prelude::*;
use factory_data::TechnologyId;

use crate::ui::formatting::format_recipe_display_name;
use crate::ui::layout::{PANEL_MARGIN, scroll_column};

use super::components::{
    ActiveResearchText, ResearchQueueText, TechnologyDetailField, TechnologyDetailRoot,
    TechnologyDetailText, TechnologyListRoot, TechnologyPanelContentRoot, TechnologyProgressFill,
    TechnologyQueueAction, TechnologyQueueButton, TechnologyQueueEmpty, TechnologyQueueRoot,
    TechnologyQueueRow, TechnologyQueueTitle, TechnologySelectButton, TechnologyStartQueueButton,
    TechnologyStatusText,
};
use super::helpers::{
    active_research_text, can_enqueue_for_ui, next_science_cost_text, prerequisite_text,
    queue_text, start_queue_label, technology_effect_text, technology_name,
    technology_progress_text, technology_state_color, technology_state_label, technology_ui_state,
};

pub(crate) fn technology_panel_root() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(PANEL_MARGIN),
            right: Val::Px(PANEL_MARGIN),
            top: Val::Px(PANEL_MARGIN),
            bottom: Val::Px(PANEL_MARGIN),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(10.0),
            padding: UiRect::all(Val::Px(18.0)),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(Color::srgba(0.025, 0.028, 0.030, 0.96)),
        GlobalZIndex(2100),
    )
}

pub(crate) fn spawn_technology_panel_contents(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    sim: &factory_sim::Simulation,
    selected: Option<TechnologyId>,
) {
    spawn_header(root, sim);
    root.spawn((
        Node {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            min_height: Val::Px(0.0),
            column_gap: Val::Px(12.0),
            row_gap: Val::Px(12.0),
            align_items: AlignItems::Stretch,
            align_content: AlignContent::Stretch,
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(Color::NONE),
        TechnologyPanelContentRoot,
    ))
    .with_children(|content| {
        spawn_technology_list(content, sim, selected);
        spawn_technology_detail(content, sim, selected);
    });
}

fn spawn_header(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    sim: &factory_sim::Simulation,
) {
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            padding: UiRect::all(Val::Px(10.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.07, 0.075, 0.075, 0.96)),
        BorderColor::all(Color::srgba(0.34, 0.36, 0.34, 0.80)),
    ))
    .with_children(|header| {
        header.spawn((
            Text::new(active_research_text(sim)),
            TextFont::from_font_size(16.0),
            TextColor(Color::WHITE),
            ActiveResearchText,
        ));
        header.spawn((
            Text::new(queue_text(sim)),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(0.78, 0.80, 0.76)),
            ResearchQueueText,
        ));
    });
}

fn spawn_technology_list(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    sim: &factory_sim::Simulation,
    selected: Option<TechnologyId>,
) {
    let mut list_node = scroll_column();
    list_node.width = Val::Percent(32.0);
    list_node.flex_basis = Val::Px(260.0);
    list_node.min_width = Val::Px(220.0);
    list_node.max_width = Val::Px(360.0);
    list_node.row_gap = Val::Px(5.0);
    list_node.padding = UiRect::all(Val::Px(8.0));
    list_node.border = UiRect::all(Val::Px(1.0));

    parent
        .spawn((
            list_node,
            BackgroundColor(Color::srgba(0.055, 0.058, 0.060, 0.96)),
            BorderColor::all(Color::srgba(0.30, 0.31, 0.30, 0.85)),
            TechnologyListRoot,
        ))
        .with_children(|list| {
            list.spawn((
                Text::new("Technologies"),
                TextFont::from_font_size(13.0),
                TextColor(Color::srgb(0.92, 0.93, 0.88)),
            ));
            for technology in sim.catalog().technologies() {
                spawn_technology_button(list, sim, technology.id, selected == Some(technology.id));
            }
        });
}

fn spawn_technology_button(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    sim: &factory_sim::Simulation,
    technology_id: TechnologyId,
    selected: bool,
) {
    let technology = sim
        .catalog()
        .technology(technology_id)
        .expect("technology list should contain valid ids");
    let state = technology_ui_state(sim, technology_id);

    parent
        .spawn((
            Button,
            Node {
                width: Val::Percent(100.0),
                min_height: Val::Px(38.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(2.0),
                padding: UiRect::all(Val::Px(6.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(technology_state_color(state)),
            BorderColor::all(if selected {
                Color::srgb(0.94, 0.66, 0.20)
            } else {
                Color::srgba(0.32, 0.33, 0.31, 0.80)
            }),
            TechnologySelectButton { technology_id },
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(format_recipe_display_name(&technology.name)),
                TextFont::from_font_size(12.0),
                TextColor(Color::WHITE),
            ));
            button.spawn((
                Text::new(technology_status_text(sim, technology_id)),
                TextFont::from_font_size(10.0),
                TextColor(Color::srgb(0.76, 0.78, 0.74)),
                TechnologyStatusText(technology_id),
            ));
        });
}

pub(crate) fn technology_status_text(
    sim: &factory_sim::Simulation,
    technology_id: TechnologyId,
) -> String {
    let state = technology_ui_state(sim, technology_id);
    let repeatable = sim
        .catalog()
        .technology(technology_id)
        .is_some_and(|technology| technology.level_model.is_repeatable());
    if repeatable {
        format!(
            "{} · Level {}",
            technology_state_label(state),
            sim.technology_level(technology_id).unwrap_or(0)
        )
    } else {
        technology_state_label(state).to_string()
    }
}

pub(crate) fn spawn_technology_detail(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    sim: &factory_sim::Simulation,
    selected: Option<TechnologyId>,
) {
    let mut detail_node = scroll_column();
    detail_node.flex_basis = Val::Px(460.0);
    detail_node.width = Val::Percent(64.0);
    detail_node.min_width = Val::Px(280.0);
    detail_node.max_width = Val::Px(820.0);
    detail_node.row_gap = Val::Px(10.0);
    detail_node.padding = UiRect::all(Val::Px(10.0));
    detail_node.border = UiRect::all(Val::Px(1.0));

    parent
        .spawn((
            detail_node,
            BackgroundColor(Color::srgba(0.055, 0.058, 0.060, 0.96)),
            BorderColor::all(Color::srgba(0.30, 0.31, 0.30, 0.85)),
            TechnologyDetailRoot,
        ))
        .with_children(|detail| {
            let Some(technology_id) = selected else {
                detail.spawn((
                    Text::new("No technology selected"),
                    TextFont::from_font_size(14.0),
                    TextColor(Color::WHITE),
                ));
                return;
            };
            let Some(technology) = sim.catalog().technology(technology_id) else {
                detail.spawn((
                    Text::new("Unknown technology"),
                    TextFont::from_font_size(14.0),
                    TextColor(Color::WHITE),
                ));
                return;
            };

            detail.spawn((
                Text::new(format_recipe_display_name(&technology.name)),
                TextFont::from_font_size(18.0),
                TextColor(Color::WHITE),
                TechnologyDetailText(TechnologyDetailField::Name),
            ));
            detail.spawn((
                Text::new(format!(
                    "Current level: {} · {}",
                    sim.technology_level(technology_id).unwrap_or(0),
                    if technology.level_model.is_repeatable() {
                        "Repeatable"
                    } else {
                        "Finite"
                    }
                )),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.84, 0.86, 0.80)),
                TechnologyDetailText(TechnologyDetailField::Level),
            ));
            detail.spawn((
                Text::new(format!(
                    "Progress: {}",
                    technology_progress_text(sim, technology_id)
                )),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.84, 0.86, 0.80)),
                TechnologyDetailText(TechnologyDetailField::Progress),
            ));
            spawn_research_progress_bar(detail, sim, technology_id);
            detail.spawn((
                Text::new(format!(
                    "Prerequisites: {}",
                    prerequisite_text(sim.catalog(), technology)
                )),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.84, 0.86, 0.80)),
                TechnologyDetailText(TechnologyDetailField::Prerequisites),
            ));
            detail.spawn((
                Text::new(format!("Cost: {}", next_science_cost_text(sim, technology))),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.84, 0.86, 0.80)),
                TechnologyDetailText(TechnologyDetailField::Cost),
            ));
            detail.spawn((
                Text::new(format!(
                    "Effects: {}",
                    technology_effect_text(sim, technology)
                )),
                TextFont::from_font_size(12.0),
                TextColor(Color::srgb(0.84, 0.86, 0.80)),
                TechnologyDetailText(TechnologyDetailField::Effects),
            ));
            spawn_start_queue_button(detail, sim, technology_id);
            spawn_queue_controls(detail, sim);
        });
}

fn spawn_research_progress_bar(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    sim: &factory_sim::Simulation,
    technology_id: TechnologyId,
) {
    parent
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.10, 0.10, 0.11, 0.96)),
        ))
        .with_child((
            Node {
                width: Val::Percent(research_progress_percent(sim, technology_id)),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.34, 0.70, 0.86)),
            TechnologyProgressFill,
        ));
}

pub(crate) fn research_progress_percent(
    sim: &factory_sim::Simulation,
    technology_id: TechnologyId,
) -> f32 {
    let progress = sim.technology_progress(technology_id).unwrap_or(0);
    sim.technology_next_required_units(technology_id)
        .map(|required| (progress as f64 * 100.0 / required.max(1) as f64).clamp(0.0, 100.0) as f32)
        .unwrap_or(100.0)
}

fn spawn_start_queue_button(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    sim: &factory_sim::Simulation,
    technology_id: TechnologyId,
) {
    let actionable = can_enqueue_for_ui(sim, technology_id);
    let label = start_queue_label(sim, technology_id);
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(160.0),
                height: Val::Px(34.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::horizontal(Val::Px(10.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(if actionable {
                Color::srgba(0.20, 0.34, 0.28, 0.98)
            } else {
                Color::srgba(0.09, 0.095, 0.095, 0.96)
            }),
            BorderColor::all(if actionable {
                Color::srgba(0.42, 0.68, 0.48, 0.85)
            } else {
                Color::srgba(0.25, 0.26, 0.25, 0.85)
            }),
            TechnologyStartQueueButton,
        ))
        .with_child((
            Text::new(label),
            TextFont::from_font_size(12.0),
            TextColor(Color::WHITE),
            TechnologyDetailText(TechnologyDetailField::Start),
        ));
}

fn spawn_queue_controls(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    sim: &factory_sim::Simulation,
) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                row_gap: Val::Px(5.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
            TechnologyQueueRoot,
        ))
        .with_children(|queue| {
            queue.spawn((
                Text::new("Pending Queue"),
                TextFont::from_font_size(13.0),
                TextColor(Color::srgb(0.92, 0.93, 0.88)),
                TechnologyQueueTitle,
            ));
            queue.spawn((
                Text::new("<empty>"),
                TextFont::from_font_size(11.0),
                TextColor(Color::srgb(0.65, 0.66, 0.62)),
                TechnologyQueueEmpty,
                if sim.research_queue().is_empty() {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
            ));

            for (index, technology_id) in sim.research_queue().iter().copied().enumerate() {
                spawn_technology_queue_row(queue, sim, index, technology_id);
            }
        });
}

pub(crate) fn spawn_technology_queue_row(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    sim: &factory_sim::Simulation,
    index: usize,
    technology_id: TechnologyId,
) -> Entity {
    let can_move_up = index > 0 && can_move_queued_research(sim, index, index - 1);
    let can_move_down =
        index + 1 < sim.research_queue().len() && can_move_queued_research(sim, index, index + 1);
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                row_gap: Val::Px(4.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
            TechnologyQueueRow(technology_id),
        ))
        .with_children(|row| {
            row.spawn((
                Text::new(format!(
                    "{}. {}",
                    index + 1,
                    technology_name(sim.catalog(), technology_id)
                )),
                TextFont::from_font_size(11.0),
                TextColor(Color::srgb(0.86, 0.88, 0.82)),
            ));
            spawn_queue_button(row, "Up", index, TechnologyQueueAction::MoveUp, can_move_up);
            spawn_queue_button(
                row,
                "Down",
                index,
                TechnologyQueueAction::MoveDown,
                can_move_down,
            );
            spawn_queue_button(row, "Remove", index, TechnologyQueueAction::Remove, true);
        })
        .id()
}

fn spawn_queue_button(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    label: &str,
    index: usize,
    action: TechnologyQueueAction,
    enabled: bool,
) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(58.0),
                height: Val::Px(24.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::horizontal(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.13, 0.14, 0.14, 0.96)),
            TechnologyQueueButton { index, action },
            if enabled {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
        ))
        .with_child((
            Text::new(label),
            TextFont::from_font_size(10.0),
            TextColor(Color::WHITE),
        ));
}

fn can_move_queued_research(
    sim: &factory_sim::Simulation,
    from_index: usize,
    to_index: usize,
) -> bool {
    sim.can_move_queued_research(from_index, to_index).is_ok()
}
