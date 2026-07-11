use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::ui_widgets::ScrollArea;
use factory_data::{BuildingCategory, PrototypeCatalog};
use factory_sim::Simulation;

use crate::audio::SoundEvent;
use crate::build::resources::{
    BuildMenuState, BuildPlacementState, BuildSelection, BuildingMenuView, HotbarState,
    PlannerState,
};
use crate::input::build::{select_build_selection, technology_window_open};
use crate::placement::build::{BuildablePrototype, buildable_prototypes, display_name};
use crate::resources::SimResource;
use crate::ui::build_bar::{BuildMenuButton, slot_key_label};
use crate::ui::resources::{OpenContainer, TechnologyWindowState};
use crate::ui::window_sync::{WindowRootQuery, sync_window};
use crate::utils::compact_item_name;

const CATEGORIES: [BuildingCategory; 6] = [
    BuildingCategory::Logistics,
    BuildingCategory::Production,
    BuildingCategory::Power,
    BuildingCategory::Fluids,
    BuildingCategory::Storage,
    BuildingCategory::Defense,
];
const CELL_WIDTH: f32 = 168.0;
const CELL_HEIGHT: f32 = 92.0;
const CELL_GAP: f32 = 7.0;
const GRID_COLUMNS: f32 = 3.0;
const GRID_PADDING: f32 = 8.0;
const GRID_WIDTH: f32 =
    GRID_COLUMNS * CELL_WIDTH + (GRID_COLUMNS - 1.0) * CELL_GAP + 2.0 * GRID_PADDING;

#[derive(Component)]
pub(crate) struct BuildMenuSelectButton {
    selection: BuildSelection,
    lock_message: Option<String>,
}

#[derive(Component)]
pub(crate) struct BuildMenuHotbarToggleButton {
    selection: BuildSelection,
}

#[derive(Component)]
pub(crate) struct BuildMenuViewButton(BuildingMenuView);

#[derive(Component)]
pub(crate) struct BuildMenuCloseButton;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BuildMenuSnapshot {
    pub(crate) entries: Vec<BuildMenuEntry>,
    pub(crate) navigation: Vec<(BuildingMenuView, usize)>,
    pub(crate) selected_view: BuildingMenuView,
    pub(crate) search_query: String,
    pub(crate) message: Option<String>,
    pub(crate) empty_message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BuildMenuEntry {
    pub(crate) selection: BuildSelection,
    pub(crate) display_name: String,
    pub(crate) internal_name: String,
    pub(crate) compact_name: String,
    pub(crate) category: BuildingCategory,
    pub(crate) progression: String,
    pub(crate) count: u32,
    pub(crate) unlocked: bool,
    pub(crate) hotbar_slot: Option<usize>,
}

type ToggleQuery<'w, 's> =
    Query<'w, 's, &'static Interaction, (Changed<Interaction>, With<BuildMenuButton>)>;
type CloseQuery<'w, 's> =
    Query<'w, 's, &'static Interaction, (Changed<Interaction>, With<BuildMenuCloseButton>)>;
type SelectQuery<'w, 's> =
    Query<'w, 's, (&'static Interaction, &'static BuildMenuSelectButton), Changed<Interaction>>;
type FavoriteQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static BuildMenuHotbarToggleButton),
    Changed<Interaction>,
>;
type ViewQuery<'w, 's> =
    Query<'w, 's, (&'static Interaction, &'static BuildMenuViewButton), Changed<Interaction>>;

#[derive(SystemParam)]
pub(crate) struct BuildMenuButtonState<'w> {
    sim: Res<'w, SimResource>,
    technology_window: Option<Res<'w, TechnologyWindowState>>,
    menu: ResMut<'w, BuildMenuState>,
    hotbar: ResMut<'w, HotbarState>,
    build_state: ResMut<'w, BuildPlacementState>,
    planner: ResMut<'w, PlannerState>,
    open_container: ResMut<'w, OpenContainer>,
    sounds: MessageWriter<'w, SoundEvent>,
}

pub(crate) fn handle_build_menu_buttons(
    mut toggles: ToggleQuery,
    mut closes: CloseQuery,
    mut selections: SelectQuery,
    mut favorites: FavoriteQuery,
    mut views: ViewQuery,
    mut state: BuildMenuButtonState,
) {
    if technology_window_open(state.technology_window.as_deref()) {
        return;
    }
    let toggle = toggles.iter_mut().any(pressed);
    let close = closes.iter_mut().any(pressed);
    if toggle || close {
        state.sounds.write(SoundEvent::UiClick);
        if state.menu.open {
            state.menu.close();
        } else if toggle {
            state.menu.open_fresh();
            state.build_state.selected = None;
            state.open_container.entity_id = None;
        }
        return;
    }
    if !state.menu.open {
        return;
    }
    for (interaction, button) in &mut views {
        if *interaction == Interaction::Pressed {
            state.menu.selected_view = button.0;
            state.menu.message = None;
            state.sounds.write(SoundEvent::UiClick);
        }
    }
    for (interaction, button) in &mut selections {
        if *interaction != Interaction::Pressed {
            continue;
        }
        state.sounds.write(SoundEvent::UiClick);
        if let Some(message) = &button.lock_message {
            state.menu.message = Some(message.clone());
            continue;
        }
        if select_build_selection(
            &state.sim.read(),
            state.technology_window.as_deref(),
            &mut state.build_state,
            &mut state.planner,
            button.selection,
        ) {
            state.menu.close();
        }
    }
    for (interaction, button) in &mut favorites {
        if *interaction != Interaction::Pressed {
            continue;
        }
        state.sounds.write(SoundEvent::UiClick);
        if state.hotbar.remove(button.selection)
            || state
                .hotbar
                .assign_to_first_empty(button.selection)
                .is_some()
        {
            state.menu.message = None;
        } else {
            state.menu.message = Some("Hotbar is full - remove a building from it first".into());
        }
    }
}

fn pressed(interaction: &Interaction) -> bool {
    *interaction == Interaction::Pressed
}

pub(crate) fn sync_build_menu(
    mut commands: Commands,
    sim: Res<SimResource>,
    hotbar: Res<HotbarState>,
    state: Res<BuildMenuState>,
    mut cached_buildables: Local<Option<Vec<BuildablePrototype>>>,
    mut roots: WindowRootQuery<BuildMenuSnapshot>,
) {
    let buildables = cached_buildables.get_or_insert_with(|| {
        let mut buildables = buildable_prototypes(sim.read().catalog());
        sort_buildables(&mut buildables);
        buildables
    });
    sync_window(
        &mut commands,
        &mut roots,
        state.open,
        sim.is_changed() || hotbar.is_changed() || state.is_changed(),
        || build_menu_snapshot(&sim.read(), &hotbar, &state, buildables),
        build_menu_root,
        spawn_contents,
    );
}

pub(crate) fn build_menu_snapshot(
    sim: &Simulation,
    hotbar: &HotbarState,
    state: &BuildMenuState,
    buildables: &[BuildablePrototype],
) -> BuildMenuSnapshot {
    let catalog = sim.catalog();
    let buildables = buildables.to_vec();
    let query = normalize(&state.search_query);
    let search_matches =
        |buildable: &BuildablePrototype| matches_search(buildable, catalog, &query);
    let mut navigation = vec![
        (
            BuildingMenuView::All,
            buildables.iter().filter(|b| search_matches(b)).count(),
        ),
        (
            BuildingMenuView::Favorites,
            buildables
                .iter()
                .filter(|b| hotbar.slot_of(b.selection()).is_some() && search_matches(b))
                .count(),
        ),
    ];
    navigation.extend(CATEGORIES.map(|category| {
        (
            BuildingMenuView::Category(category),
            buildables
                .iter()
                .filter(|b| b.category == category && search_matches(b))
                .count(),
        )
    }));
    let entries = buildables
        .into_iter()
        .filter(|buildable| match state.selected_view {
            BuildingMenuView::All => true,
            BuildingMenuView::Favorites => hotbar.slot_of(buildable.selection()).is_some(),
            BuildingMenuView::Category(category) => buildable.category == category,
        })
        .filter(search_matches)
        .map(|buildable| entry_from_buildable(sim, hotbar, catalog, buildable))
        .collect::<Vec<_>>();
    let empty_message = entries.is_empty().then(|| empty_message(state, &query));
    BuildMenuSnapshot {
        entries,
        navigation,
        selected_view: state.selected_view,
        search_query: state.search_query.clone(),
        message: state.message.clone(),
        empty_message,
    }
}

fn sort_buildables(buildables: &mut [BuildablePrototype]) {
    buildables.sort_by(|left, right| {
        left.category
            .cmp(&right.category)
            .then_with(|| left.menu_order.cmp(&right.menu_order))
            .then_with(|| normalize(&left.display_name).cmp(&normalize(&right.display_name)))
            .then_with(|| left.prototype_id.cmp(&right.prototype_id))
    });
}

fn entry_from_buildable(
    sim: &Simulation,
    hotbar: &HotbarState,
    catalog: &PrototypeCatalog,
    buildable: BuildablePrototype,
) -> BuildMenuEntry {
    let selection = buildable.selection();
    let required = buildable
        .required_technology
        .and_then(|id| catalog.technology(id))
        .map(|technology| display_name(&technology.name));
    let unlocked = sim.is_entity_unlocked(selection.prototype_id);
    let progression = required.unwrap_or_else(|| {
        if unlocked {
            "Starter"
        } else {
            "Technology required"
        }
        .to_string()
    });
    BuildMenuEntry {
        selection,
        internal_name: catalog
            .entity(selection.prototype_id)
            .map_or_else(String::new, |e| e.name.clone()),
        compact_name: compact_item_name(&buildable.display_name.to_lowercase().replace(' ', "_")),
        display_name: buildable.display_name,
        category: buildable.category,
        progression,
        count: sim.player_inventory().count(selection.item_id),
        unlocked,
        hotbar_slot: hotbar.slot_of(selection),
    }
}

fn matches_search(buildable: &BuildablePrototype, catalog: &PrototypeCatalog, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let internal = catalog
        .entity(buildable.prototype_id)
        .map_or("", |e| e.name.as_str());
    let technology = buildable
        .required_technology
        .and_then(|id| catalog.technology(id))
        .map_or_else(String::new, |technology| display_name(&technology.name));
    [
        buildable.display_name.as_str(),
        internal,
        category_name(buildable.category),
        technology.as_str(),
    ]
    .iter()
    .any(|candidate| normalize(candidate).contains(query))
}

fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn empty_message(state: &BuildMenuState, query: &str) -> String {
    let view = view_name(state.selected_view);
    if query.is_empty() {
        format!("No buildings in {view}.")
    } else {
        format!(
            "No buildings match ‘{}’ in {view}.",
            state.search_query.trim()
        )
    }
}

fn category_name(category: BuildingCategory) -> &'static str {
    match category {
        BuildingCategory::Logistics => "Logistics",
        BuildingCategory::Production => "Production",
        BuildingCategory::Power => "Power",
        BuildingCategory::Fluids => "Fluids",
        BuildingCategory::Storage => "Storage",
        BuildingCategory::Defense => "Defense",
    }
}

fn view_name(view: BuildingMenuView) -> &'static str {
    match view {
        BuildingMenuView::All => "All",
        BuildingMenuView::Favorites => "Favorites",
        BuildingMenuView::Category(category) => category_name(category),
    }
}

fn build_menu_root() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::ZERO,
            right: Val::ZERO,
            top: Val::ZERO,
            bottom: Val::ZERO,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.48)),
        GlobalZIndex(2200),
    )
}

fn spawn_contents(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &BuildMenuSnapshot,
) {
    root.spawn((Node { width: Val::Px(750.0), max_width: Val::Vw(92.0), height: Val::Vh(72.0), max_height: Val::Px(720.0), flex_direction: FlexDirection::Column, row_gap: Val::Px(10.0), padding: UiRect::all(Val::Px(14.0)), border: UiRect::all(Val::Px(1.0)), overflow: Overflow::clip(), ..default() }, BackgroundColor(Color::srgba(0.035, 0.038, 0.040, 0.98)), BorderColor::all(Color::srgba(0.49, 0.48, 0.42, 0.8))))
        .with_children(|panel| {
            spawn_header(panel, snapshot);
            panel.spawn((Node { flex_grow: 1.0, flex_direction: FlexDirection::Row, column_gap: Val::Px(12.0), min_height: Val::ZERO, ..default() }, BackgroundColor(Color::NONE))).with_children(|body| {
                spawn_navigation(body, snapshot);
                body.spawn((Node { flex_grow: 1.0, min_width: Val::Px(GRID_WIDTH), height: Val::Percent(100.0), overflow: Overflow::scroll_y(), scrollbar_width: 10.0, padding: UiRect::right(Val::Px(4.0)), ..default() }, BackgroundColor(Color::srgba(0.02, 0.022, 0.023, 0.75)), ScrollArea)).with_children(|viewport| {
                    viewport.spawn((Node { width: Val::Px(GRID_WIDTH), flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, align_content: AlignContent::FlexStart, column_gap: Val::Px(CELL_GAP), row_gap: Val::Px(CELL_GAP), padding: UiRect::all(Val::Px(GRID_PADDING)), ..default() }, BackgroundColor(Color::NONE))).with_children(|grid| {
                        if let Some(message) = &snapshot.empty_message {
                            grid.spawn((Text::new(message.clone()), TextFont::from_font_size(14.0), TextColor(Color::srgb(0.72, 0.72, 0.67))));
                        }
                        for entry in &snapshot.entries { spawn_entry(grid, entry); }
                    });
                });
            });
            if let Some(message) = &snapshot.message { panel.spawn((Text::new(message.clone()), TextFont::from_font_size(12.0), TextColor(Color::srgb(0.98, 0.72, 0.28)))); }
            panel.spawn((Text::new("Click an unlocked card to build • ★ toggles hotbar favorite • 1–0 select favorites outside catalog"), TextFont::from_font_size(11.0), TextColor(Color::srgb(0.68, 0.70, 0.66))));
        });
}

fn spawn_header(
    panel: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &BuildMenuSnapshot,
) {
    panel
        .spawn((
            Node {
                height: Val::Px(42.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(14.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_children(|header| {
            header.spawn((
                Text::new("Building Catalog"),
                TextFont::from_font_size(20.0),
                TextColor(Color::srgb(0.94, 0.93, 0.86)),
            ));
            let search = if snapshot.search_query.is_empty() {
                "Search buildings…".into()
            } else {
                snapshot.search_query.clone()
            };
            header
                .spawn((
                    Node {
                        flex_grow: 1.0,
                        height: Val::Px(30.0),
                        align_items: AlignItems::Center,
                        padding: UiRect::horizontal(Val::Px(10.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.07, 0.08, 0.08, 0.96)),
                    BorderColor::all(Color::srgba(0.48, 0.55, 0.48, 0.8)),
                ))
                .with_child((
                    Text::new(search),
                    TextFont::from_font_size(12.0),
                    TextColor(Color::srgb(0.78, 0.82, 0.75)),
                ));
            header
                .spawn((
                    Button,
                    Node {
                        height: Val::Px(28.0),
                        padding: UiRect::horizontal(Val::Px(10.0)),
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.15, 0.15, 0.15, 0.95)),
                    BorderColor::all(Color::srgba(0.44, 0.43, 0.39, 0.7)),
                    BuildMenuCloseButton,
                ))
                .with_child((
                    Text::new("Close (Esc)"),
                    TextFont::from_font_size(11.0),
                    TextColor(Color::WHITE),
                ));
        });
}

fn spawn_navigation(
    body: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    snapshot: &BuildMenuSnapshot,
) {
    body.spawn((
        Node {
            width: Val::Px(145.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(5.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
    ))
    .with_children(|rail| {
        for (view, count) in &snapshot.navigation {
            let selected = *view == snapshot.selected_view;
            let favorite = *view == BuildingMenuView::Favorites;
            rail.spawn((
                Button,
                Node {
                    height: Val::Px(34.0),
                    padding: UiRect::horizontal(Val::Px(9.0)),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    border: UiRect::left(Val::Px(if selected { 4.0 } else { 1.0 })),
                    ..default()
                },
                BackgroundColor(if selected {
                    Color::srgba(0.18, 0.30, 0.22, 0.98)
                } else {
                    Color::srgba(0.09, 0.10, 0.10, 0.95)
                }),
                BorderColor::all(if favorite {
                    Color::srgb(0.94, 0.66, 0.20)
                } else {
                    Color::srgb(0.38, 0.62, 0.44)
                }),
                BuildMenuViewButton(*view),
            ))
            .with_children(|button| {
                button.spawn((
                    Text::new(if favorite {
                        format!("★ {}", view_name(*view))
                    } else {
                        view_name(*view).into()
                    }),
                    TextFont::from_font_size(12.0),
                    TextColor(Color::srgb(0.88, 0.89, 0.83)),
                ));
                button.spawn((
                    Text::new(count.to_string()),
                    TextFont::from_font_size(11.0),
                    TextColor(Color::srgb(0.64, 0.68, 0.62)),
                ));
            });
        }
    });
}

fn spawn_entry(grid: &mut bevy::ecs::hierarchy::ChildSpawnerCommands, entry: &BuildMenuEntry) {
    let favorite = entry.hotbar_slot.is_some();
    let border = if favorite {
        Color::srgba(0.94, 0.66, 0.20, 0.85)
    } else {
        Color::srgba(0.44, 0.43, 0.39, 0.72)
    };
    let lock_message = (!entry.unlocked).then(|| {
        if entry.progression == "Technology required" {
            entry.progression.clone()
        } else {
            format!("Requires {}", entry.progression)
        }
    });
    grid.spawn((
        Node {
            width: Val::Px(CELL_WIDTH),
            height: Val::Px(CELL_HEIGHT),
            flex_direction: FlexDirection::Row,
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::NONE),
        BorderColor::all(border),
    ))
    .with_children(|cell| {
        cell.spawn((
            Button,
            Node {
                flex_grow: 1.0,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::all(Val::Px(7.0)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(if entry.unlocked {
                Color::srgba(0.13, 0.14, 0.13, 0.97)
            } else {
                Color::srgba(0.07, 0.075, 0.07, 0.94)
            }),
            BuildMenuSelectButton {
                selection: entry.selection,
                lock_message,
            },
        ))
        .with_children(|button| {
            let key = entry
                .hotbar_slot
                .map(|slot| format!(" [{}]", slot_key_label(slot)))
                .unwrap_or_default();
            button.spawn((
                Text::new(format!("{}{}", entry.compact_name, key)),
                TextFont::from_font_size(12.0),
                TextColor(if entry.unlocked {
                    Color::WHITE
                } else {
                    Color::srgb(0.58, 0.58, 0.54)
                }),
            ));
            button.spawn((
                Text::new(entry.display_name.clone()),
                TextFont::from_font_size(9.0),
                TextColor(Color::srgb(0.67, 0.70, 0.64)),
            ));
            button.spawn((
                Text::new(if entry.unlocked {
                    format!("Inventory: {}", entry.count)
                } else {
                    "Locked".into()
                }),
                TextFont::from_font_size(10.0),
                TextColor(Color::srgb(0.78, 0.74, 0.64)),
            ));
            button.spawn((
                Text::new(entry.progression.clone()),
                TextFont::from_font_size(9.0),
                TextColor(Color::srgb(0.48, 0.70, 0.52)),
            ));
        });
        cell.spawn((
            Button,
            Node {
                width: Val::Px(27.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                border: UiRect::left(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(if favorite {
                Color::srgba(0.27, 0.20, 0.10, 0.98)
            } else {
                Color::srgba(0.09, 0.11, 0.09, 0.98)
            }),
            BorderColor::all(border),
            BuildMenuHotbarToggleButton {
                selection: entry.selection,
            },
        ))
        .with_child((
            Text::new(if favorite { "★" } else { "☆" }),
            TextFont::from_font_size(18.0),
            TextColor(if favorite {
                Color::srgb(0.98, 0.72, 0.28)
            } else {
                Color::srgb(0.65, 0.72, 0.63)
            }),
        ));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_trims_collapses_and_lowercases() {
        assert_eq!(normalize("  Fast   BELT  "), "fast belt");
    }

    #[test]
    fn snapshots_filter_views_search_and_keep_locked_entries() {
        let sim = Simulation::new(
            7,
            PrototypeCatalog::load_base().expect("base catalog should load"),
        );
        let mut buildables = buildable_prototypes(sim.catalog());
        sort_buildables(&mut buildables);
        let catalog_count = buildables.len();
        let mut hotbar = HotbarState::default();
        let favorite = buildables[0].selection();
        hotbar.slots[4] = Some(favorite);
        let mut state = BuildMenuState::default();

        let all = build_menu_snapshot(&sim, &hotbar, &state, &buildables);
        assert_eq!(all.entries.len(), catalog_count);
        assert!(all.entries.iter().any(|entry| !entry.unlocked));

        state.selected_view = BuildingMenuView::Favorites;
        assert_eq!(
            build_menu_snapshot(&sim, &hotbar, &state, &buildables)
                .entries
                .len(),
            1
        );
        state.selected_view = BuildingMenuView::Category(BuildingCategory::Defense);
        assert!(
            build_menu_snapshot(&sim, &hotbar, &state, &buildables)
                .entries
                .iter()
                .all(|entry| entry.category == BuildingCategory::Defense)
        );

        state.selected_view = BuildingMenuView::All;
        state.search_query = "LoGiStIcS".into();
        assert!(
            build_menu_snapshot(&sim, &hotbar, &state, &buildables)
                .entries
                .iter()
                .all(|entry| entry.category == BuildingCategory::Logistics)
        );
        state.search_query = "automation".into();
        assert!(
            build_menu_snapshot(&sim, &hotbar, &state, &buildables)
                .entries
                .iter()
                .any(|entry| entry.internal_name == "assembling_machine")
        );
        state.search_query = "does not exist".into();
        assert!(
            build_menu_snapshot(&sim, &hotbar, &state, &buildables)
                .empty_message
                .is_some()
        );
    }
}
