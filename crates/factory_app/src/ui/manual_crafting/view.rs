use bevy::prelude::*;

use crate::resources::CraftingPanelTab;

use super::components::{
    CraftingPanelRoot, CraftingPanelSnapshot, CraftingRecipeButton, CraftingTabButton,
    ManualCraftRecipeRow,
};

pub(crate) fn spawn_manual_crafting_panel(
    commands: &mut Commands,
    snapshot: CraftingPanelSnapshot,
) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                width: Val::Px(520.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(10.0),
                padding: UiRect::all(Val::Px(16.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.028, 0.030, 0.032, 0.97)),
            GlobalZIndex(2400),
            CraftingPanelRoot {
                snapshot: snapshot.clone(),
            },
        ))
        .with_children(|root| spawn_manual_crafting_contents(root, &snapshot));
}

pub(crate) fn spawn_manual_crafting_contents(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &CraftingPanelSnapshot,
) {
    root.spawn((
        Text::new("Crafting"),
        TextFont::from_font_size(18.0),
        TextColor(Color::srgb(0.94, 0.95, 0.90)),
    ));
    spawn_tabs(root, snapshot.selected_tab);
    spawn_recipe_rows(root, &snapshot.rows);
    spawn_queue(root, &snapshot.queue);
}

fn spawn_tabs(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands, selected: CraftingPanelTab) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(6.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|tabs| {
            for (tab, label) in [
                (CraftingPanelTab::Player, "Player"),
                (CraftingPanelTab::Smelting, "Smelting"),
                (CraftingPanelTab::Assembling, "Assembling"),
            ] {
                tabs.spawn((
                    Button,
                    Node {
                        height: Val::Px(32.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        padding: UiRect::horizontal(Val::Px(12.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(if tab == selected {
                        Color::srgba(0.22, 0.27, 0.24, 0.98)
                    } else {
                        Color::srgba(0.10, 0.11, 0.11, 0.98)
                    }),
                    BorderColor::all(Color::srgba(0.38, 0.42, 0.36, 0.85)),
                    CraftingTabButton { tab },
                ))
                .with_child((
                    Text::new(label),
                    TextFont::from_font_size(12.0),
                    TextColor(Color::WHITE),
                ));
            }
        });
}

fn spawn_recipe_rows(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    rows: &[ManualCraftRecipeRow],
) {
    parent.spawn((
        Text::new("Recipes"),
        TextFont::from_font_size(14.0),
        TextColor(Color::srgb(0.92, 0.93, 0.88)),
    ));

    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(5.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|list| {
            if rows.is_empty() {
                list.spawn((
                    Text::new("<none>"),
                    TextFont::from_font_size(12.0),
                    TextColor(Color::srgb(0.62, 0.64, 0.60)),
                ));
                return;
            }

            for row in rows {
                spawn_recipe_row(list, row);
            }
        });
}

fn spawn_recipe_row(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    row: &ManualCraftRecipeRow,
) {
    parent
        .spawn((
            Node {
                width: Val::Percent(100.0),
                min_height: Val::Px(58.0),
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(8.0),
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(7.0)),
                ..default()
            },
            BackgroundColor(if row.button_enabled {
                Color::srgba(0.070, 0.077, 0.073, 0.94)
            } else {
                Color::srgba(0.050, 0.052, 0.052, 0.90)
            }),
        ))
        .with_children(|row_node| {
            row_node
                .spawn((
                    Node {
                        width: Val::Px(330.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(2.0),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                ))
                .with_children(|details| {
                    details.spawn((
                        Text::new(row.display_name.clone()),
                        TextFont::from_font_size(12.0),
                        TextColor(Color::WHITE),
                    ));
                    details.spawn((
                        Text::new(row.products.clone()),
                        TextFont::from_font_size(10.0),
                        TextColor(Color::srgb(0.82, 0.84, 0.78)),
                    ));
                    details.spawn((
                        Text::new(row.ingredients.clone()),
                        TextFont::from_font_size(10.0),
                        TextColor(Color::srgb(0.76, 0.78, 0.73)),
                    ));
                });

            if row.button_enabled {
                row_node
                    .spawn((
                        Button,
                        Node {
                            width: Val::Px(130.0),
                            min_height: Val::Px(34.0),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            padding: UiRect::horizontal(Val::Px(8.0)),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.18, 0.34, 0.25, 0.98)),
                        BorderColor::all(Color::srgba(0.42, 0.55, 0.43, 0.90)),
                        CraftingRecipeButton {
                            recipe_id: row.recipe_id,
                        },
                    ))
                    .with_child((
                        Text::new(row.status.clone()),
                        TextFont::from_font_size(11.0),
                        TextColor(Color::WHITE),
                        TextLayout::justify(Justify::Center),
                    ));
            } else {
                row_node.spawn((
                    Node {
                        width: Val::Px(130.0),
                        min_height: Val::Px(34.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        padding: UiRect::horizontal(Val::Px(8.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.09, 0.095, 0.095, 0.96)),
                    BorderColor::all(Color::srgba(0.25, 0.26, 0.25, 0.85)),
                    Text::new(row.status.clone()),
                    TextFont::from_font_size(10.0),
                    TextColor(Color::srgb(0.72, 0.74, 0.70)),
                    TextLayout::justify(Justify::Center),
                ));
            }
        });
}

fn spawn_queue(parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands, queue: &[String]) {
    parent.spawn((
        Text::new("Queue"),
        TextFont::from_font_size(14.0),
        TextColor(Color::srgb(0.92, 0.93, 0.88)),
    ));

    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                padding: UiRect::top(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|queue_node| {
            if queue.is_empty() {
                queue_node.spawn((
                    Text::new("<empty>"),
                    TextFont::from_font_size(12.0),
                    TextColor(Color::srgb(0.62, 0.64, 0.60)),
                ));
                return;
            }

            for line in queue {
                queue_node.spawn((
                    Text::new(line.clone()),
                    TextFont::from_font_size(12.0),
                    TextColor(Color::srgb(0.84, 0.86, 0.80)),
                ));
            }
        });
}
