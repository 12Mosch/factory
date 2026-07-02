use bevy::prelude::*;
use factory_data::{PrototypeCatalog, RecipeId};
use factory_sim::AssemblingMachineState;

use crate::audio::SoundEvent;
use crate::constants::{MACHINE_BAR_HEIGHT, MACHINE_BAR_WIDTH};
use crate::interaction::machine_kind::{OpenMachineKind, open_machine_kind};
use crate::resources::{OpenContainer, SimResource};
use crate::ui::formatting::{
    AssemblerDetailText, crafting_recipe_choices, format_assembler_detail_text,
    format_recipe_display_name,
};
use crate::ui::inventory_panel::{InventoryPanel, spawn_slot_button};
use crate::ui::machine_indicators::BurnerProgressFill;

#[derive(Component)]
pub(crate) struct AssemblerRecipeButton {
    recipe_id: RecipeId,
}

#[derive(Component)]
pub(crate) struct AssemblerRecipeText;

#[derive(Component)]
pub(crate) struct AssemblerIngredientsText;

#[derive(Component)]
pub(crate) struct AssemblerProductsText;

#[derive(Component)]
pub(crate) struct AssemblerProgressText;

pub(crate) type AssemblerRecipeButtonInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static AssemblerRecipeButton),
    (Changed<Interaction>, With<Button>),
>;
pub(crate) type AssemblerDetailTextQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Text,
        Has<AssemblerRecipeText>,
        Has<AssemblerIngredientsText>,
        Has<AssemblerProductsText>,
        Has<AssemblerProgressText>,
    ),
    Or<(
        With<AssemblerRecipeText>,
        With<AssemblerIngredientsText>,
        With<AssemblerProductsText>,
        With<AssemblerProgressText>,
    )>,
>;

pub(crate) fn spawn_assembler_panel(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    catalog: &PrototypeCatalog,
    state: &AssemblingMachineState,
) {
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            width: Val::Px(420.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ))
    .with_children(|panel| {
        panel.spawn((
            Text::new("Assembling Machine 1"),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
        ));
        panel.spawn((
            Text::new("Recipe: <none>"),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(0.86, 0.88, 0.82)),
            AssemblerRecipeText,
        ));
        panel
            .spawn((
                Node {
                    width: Val::Px(420.0),
                    flex_wrap: FlexWrap::Wrap,
                    row_gap: Val::Px(4.0),
                    column_gap: Val::Px(4.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|recipes| {
                for recipe in crafting_recipe_choices(catalog) {
                    spawn_assembler_recipe_button(recipes, recipe.id, &recipe.name);
                }
            });
        panel.spawn((
            Text::new("Ingredients: <none>"),
            TextFont::from_font_size(11.0),
            TextColor(Color::srgb(0.86, 0.88, 0.82)),
            AssemblerIngredientsText,
        ));
        panel.spawn((
            Text::new("Output: <none>"),
            TextFont::from_font_size(11.0),
            TextColor(Color::srgb(0.86, 0.88, 0.82)),
            AssemblerProductsText,
        ));
        panel.spawn((
            Text::new("Progress: 0/0"),
            TextFont::from_font_size(11.0),
            TextColor(Color::srgb(0.86, 0.88, 0.82)),
            AssemblerProgressText,
        ));
        panel
            .spawn((
                Node {
                    width: Val::Px(MACHINE_BAR_WIDTH),
                    height: Val::Px(MACHINE_BAR_HEIGHT),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.10, 0.10, 0.11, 0.96)),
            ))
            .with_child((
                Node {
                    width: Val::Px(0.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.34, 0.70, 0.86)),
                BurnerProgressFill,
            ));
        panel
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|groups| {
                groups.spawn((
                    Text::new("Input"),
                    TextFont::from_font_size(11.0),
                    TextColor(Color::srgb(0.78, 0.80, 0.78)),
                ));
                groups
                    .spawn((
                        Node {
                            flex_wrap: FlexWrap::Wrap,
                            row_gap: Val::Px(4.0),
                            column_gap: Val::Px(4.0),
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                    ))
                    .with_children(|slots| {
                        for slot_index in 0..state.input_inventory.slots.len() {
                            spawn_slot_button(slots, InventoryPanel::AssemblerInput, slot_index);
                        }
                    });
                groups.spawn((
                    Text::new("Output"),
                    TextFont::from_font_size(11.0),
                    TextColor(Color::srgb(0.78, 0.80, 0.78)),
                ));
                groups
                    .spawn((
                        Node {
                            flex_wrap: FlexWrap::Wrap,
                            row_gap: Val::Px(4.0),
                            column_gap: Val::Px(4.0),
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                    ))
                    .with_children(|slots| {
                        for slot_index in 0..state.output_inventory.slots.len() {
                            spawn_slot_button(slots, InventoryPanel::AssemblerOutput, slot_index);
                        }
                    });
            });
    });
}

pub(crate) fn spawn_assembler_recipe_button(
    parent: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    recipe_id: RecipeId,
    recipe_name: &str,
) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(132.0),
                height: Val::Px(38.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(assembler_recipe_button_normal_color()),
            AssemblerRecipeButton { recipe_id },
        ))
        .with_child((
            Text::new(format_recipe_button_label(recipe_name)),
            TextFont::from_font_size(9.0),
            TextColor(Color::WHITE),
            TextLayout::justify(Justify::Center),
        ));
}

pub(crate) fn handle_assembler_recipe_button_clicks(
    mut interactions: AssemblerRecipeButtonInteractionQuery,
    mut sim: ResMut<SimResource>,
    open_container: Res<OpenContainer>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let Some(entity_id) = open_container.entity_id else {
        return;
    };
    if open_machine_kind(&sim.sim, entity_id) != Some(OpenMachineKind::Assembler) {
        return;
    }

    for (interaction, button) in &mut interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }

        if sim
            .sim
            .select_assembler_recipe(entity_id, button.recipe_id)
            .is_ok()
        {
            sounds.write(SoundEvent::UiClick);
        }
    }
}

pub(crate) fn update_assembler_detail_text(
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut texts: AssemblerDetailTextQuery,
) {
    let details = open_container
        .entity_id
        .and_then(|entity_id| format_assembler_detail_text(&sim.sim, entity_id))
        .unwrap_or_else(AssemblerDetailText::empty);

    for (mut text, is_recipe, is_ingredients, is_products, is_progress) in &mut texts {
        if is_recipe {
            text.0 = details.recipe.clone();
        } else if is_ingredients {
            text.0 = details.ingredients.clone();
        } else if is_products {
            text.0 = details.products.clone();
        } else if is_progress {
            text.0 = details.progress.clone();
        }
    }
}

pub(crate) fn update_assembler_recipe_button_colors(
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut buttons: Query<(&AssemblerRecipeButton, &mut BackgroundColor)>,
) {
    let Some(entity_id) = open_container.entity_id else {
        return;
    };
    let selected_recipe = sim
        .sim
        .assembler_state(entity_id)
        .ok()
        .and_then(|state| state.selected_recipe);

    for (button, mut color) in &mut buttons {
        color.0 = if selected_recipe == Some(button.recipe_id) {
            assembler_recipe_button_selected_color()
        } else if sim
            .sim
            .can_select_assembler_recipe(entity_id, button.recipe_id)
            .unwrap_or(false)
        {
            assembler_recipe_button_normal_color()
        } else {
            assembler_recipe_button_muted_color()
        };
    }
}

pub(crate) fn format_recipe_button_label(name: &str) -> String {
    format_recipe_display_name(name)
}

pub(crate) fn assembler_recipe_button_normal_color() -> Color {
    Color::srgba(0.16, 0.18, 0.18, 0.96)
}

pub(crate) fn assembler_recipe_button_selected_color() -> Color {
    Color::srgba(0.18, 0.43, 0.55, 0.98)
}

pub(crate) fn assembler_recipe_button_muted_color() -> Color {
    Color::srgba(0.08, 0.09, 0.09, 0.96)
}
