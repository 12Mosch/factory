use bevy::prelude::*;
use factory_data::{CraftingCategory, PrototypeCatalog, RecipeId};
use factory_sim::{InventoryPanel, SimCommand};

use crate::constants::{MACHINE_BAR_HEIGHT, MACHINE_BAR_WIDTH};
use crate::interaction::machine_kind::{OpenMachineKind, open_machine_kind};
use crate::resources::SimResource;
use crate::simulation::SimCommandRequest;
use crate::ui::formatting::{
    CraftingDetailText, format_crafting_detail_text, format_recipe_display_name,
    machine_recipe_choices,
};
use crate::ui::inventory_panel::spawn_slot_button;
use crate::ui::machine_indicators::MachineProgressFill;
use crate::ui::resources::OpenContainer;

/// What one crafting machine's window shows.
///
/// Assembling machines and rocket silos share this panel because they share the
/// thing it is a window onto: ingredients going in, a recipe, progress toward
/// the next one. The two fields that are optional are exactly the two things a
/// silo does not have — a recipe to choose and an output slot to empty.
pub(crate) struct CraftingPanelSpec<'a> {
    pub(crate) title: &'a str,
    /// Slots the machine's ingredients sit in.
    pub(crate) input: CraftingPanelSlots,
    /// Slots finished products sit in, or `None` for a machine whose product is
    /// not an item in a slot.
    pub(crate) output: Option<CraftingPanelSlots>,
    /// Label for the secondary slot group (normally output, cargo for silos).
    pub(crate) output_label: &'a str,
    /// Category whose recipes the player may pick between, or `None` for a
    /// machine whose recipe is fixed.
    pub(crate) selectable_category: Option<CraftingCategory>,
}

#[derive(Clone, Copy)]
pub(crate) struct CraftingPanelSlots {
    pub(crate) panel: InventoryPanel,
    pub(crate) count: usize,
}

#[derive(Component)]
pub(crate) struct CraftingRecipeButton {
    recipe_id: RecipeId,
}

#[derive(Component)]
pub(crate) struct CraftingRecipeText;

#[derive(Component)]
pub(crate) struct CraftingIngredientsText;

#[derive(Component)]
pub(crate) struct CraftingProductsText;

#[derive(Component)]
pub(crate) struct CraftingProgressText;

pub(crate) type CraftingRecipeButtonInteractionQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static CraftingRecipeButton),
    (Changed<Interaction>, With<Button>),
>;
pub(crate) type CraftingDetailTextQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Text,
        Has<CraftingRecipeText>,
        Has<CraftingIngredientsText>,
        Has<CraftingProductsText>,
        Has<CraftingProgressText>,
    ),
    Or<(
        With<CraftingRecipeText>,
        With<CraftingIngredientsText>,
        With<CraftingProductsText>,
        With<CraftingProgressText>,
    )>,
>;

pub(crate) fn spawn_crafting_panel(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    catalog: &PrototypeCatalog,
    spec: CraftingPanelSpec<'_>,
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
            Text::new(spec.title.to_string()),
            TextFont::from_font_size(14.0),
            TextColor(Color::WHITE),
        ));
        panel.spawn((
            Text::new("Recipe: <none>"),
            TextFont::from_font_size(12.0),
            TextColor(Color::srgb(0.86, 0.88, 0.82)),
            CraftingRecipeText,
        ));
        if let Some(category) = spec.selectable_category {
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
                    for recipe in machine_recipe_choices(catalog, category) {
                        spawn_crafting_recipe_button(recipes, recipe.id, &recipe.name);
                    }
                });
        }
        panel.spawn((
            Text::new("Ingredients: <none>"),
            TextFont::from_font_size(11.0),
            TextColor(Color::srgb(0.86, 0.88, 0.82)),
            CraftingIngredientsText,
        ));
        panel.spawn((
            Text::new("Output: <none>"),
            TextFont::from_font_size(11.0),
            TextColor(Color::srgb(0.86, 0.88, 0.82)),
            CraftingProductsText,
        ));
        panel.spawn((
            Text::new("Progress: 0/0"),
            TextFont::from_font_size(11.0),
            TextColor(Color::srgb(0.86, 0.88, 0.82)),
            CraftingProgressText,
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
                MachineProgressFill,
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
                spawn_slot_group(groups, "Input", spec.input);
                if let Some(output) = spec.output {
                    spawn_slot_group(groups, spec.output_label, output);
                }
            });
    });
}

fn spawn_slot_group(
    groups: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    label: &str,
    slots: CraftingPanelSlots,
) {
    groups.spawn((
        Text::new(label.to_string()),
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
        .with_children(|buttons| {
            for slot_index in 0..slots.count {
                spawn_slot_button(buttons, slots.panel, slot_index);
            }
        });
}

pub(crate) fn spawn_crafting_recipe_button(
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
            BackgroundColor(crafting_recipe_button_normal_color()),
            CraftingRecipeButton { recipe_id },
        ))
        .with_child((
            Text::new(format_recipe_button_label(recipe_name)),
            TextFont::from_font_size(9.0),
            TextColor(Color::WHITE),
            TextLayout::justify(Justify::Center),
        ));
}

pub(crate) fn handle_crafting_recipe_button_clicks(
    mut interactions: CraftingRecipeButtonInteractionQuery,
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut commands: MessageWriter<SimCommandRequest>,
) {
    let Some(entity_id) = open_container.entity_id else {
        return;
    };
    // Recipe buttons only ever exist for an assembler; a silo's panel spawns
    // none, so a click that arrives while one is open is a stale button.
    if open_machine_kind(&sim.read(), entity_id) != Some(OpenMachineKind::Assembler) {
        return;
    }

    for (interaction, button) in &mut interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }

        commands.write(SimCommandRequest(SimCommand::SelectAssemblerRecipe {
            entity_id,
            recipe_id: button.recipe_id,
        }));
    }
}

pub(crate) fn update_crafting_detail_text(
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut texts: CraftingDetailTextQuery,
) {
    let details = open_container
        .entity_id
        .and_then(|entity_id| format_crafting_detail_text(&sim.read(), entity_id))
        .unwrap_or_else(CraftingDetailText::empty);

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

pub(crate) fn update_crafting_recipe_button_colors(
    sim: Res<SimResource>,
    open_container: Res<OpenContainer>,
    mut buttons: Query<(&CraftingRecipeButton, &mut BackgroundColor)>,
) {
    let Some(entity_id) = open_container.entity_id else {
        return;
    };
    let selected_recipe = factory_sim::entity_access::assembler_state(&sim.read(), entity_id)
        .ok()
        .and_then(|state| state.selected_recipe);

    for (button, mut color) in &mut buttons {
        color.0 = if selected_recipe == Some(button.recipe_id) {
            crafting_recipe_button_selected_color()
        } else if sim
            .read()
            .can_select_assembler_recipe(entity_id, button.recipe_id)
            .unwrap_or(false)
        {
            crafting_recipe_button_normal_color()
        } else {
            crafting_recipe_button_muted_color()
        };
    }
}

pub(crate) fn format_recipe_button_label(name: &str) -> String {
    format_recipe_display_name(name)
}

pub(crate) fn crafting_recipe_button_normal_color() -> Color {
    Color::srgba(0.16, 0.18, 0.18, 0.96)
}

pub(crate) fn crafting_recipe_button_selected_color() -> Color {
    Color::srgba(0.18, 0.43, 0.55, 0.98)
}

pub(crate) fn crafting_recipe_button_muted_color() -> Color {
    Color::srgba(0.08, 0.09, 0.09, 0.96)
}
