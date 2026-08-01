//! The grid of signals a slot is filled from, shared by every editor that asks
//! the player to name one.
//!
//! Picking "which item", "which fluid", and "which signal channel" are the same
//! interaction wherever they are asked, so the window is built once here and its
//! callers differ only in the marker their cells carry — a circuit slot in one
//! panel, a train's wait condition in another. Keeping the marker generic is
//! what lets two pickers exist without their button handlers seeing each
//! other's presses.

use bevy::prelude::*;
use bevy::ui_widgets::ScrollArea;
use factory_data::PrototypeCatalog;
use factory_sim::SignalId;

use crate::ui::circuit::signals::{SignalCatalog, signal_short_label};
use crate::ui::circuit::widgets::BUTTON_BACKGROUND;

const CELL_WIDTH: f32 = 42.0;
const CELL_HEIGHT: f32 = 20.0;

/// Which of the catalog's three channel families a section holds.
///
/// The loop below is keyed by this rather than by the heading it prints, so
/// renaming a heading cannot silently change what a filter accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SignalSection {
    Items,
    Fluids,
    Virtuals,
}

/// Which channels a slot can actually hold.
///
/// A chest row can only hold an item and a fluid wait condition can only hold a
/// fluid, so offering the other families would be offering something the slot
/// could never accept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SignalFilter {
    Any,
    ItemsOnly,
    FluidsOnly,
}

impl SignalFilter {
    const fn accepts(self, section: SignalSection) -> bool {
        match self {
            Self::Any => true,
            Self::ItemsOnly => matches!(section, SignalSection::Items),
            Self::FluidsOnly => matches!(section, SignalSection::Fluids),
        }
    }

    /// What the window calls itself, which is the shortest way to say what it
    /// will accept before the player goes looking for it.
    pub(crate) const fn title(self) -> &'static str {
        match self {
            Self::Any => "Pick a signal",
            Self::ItemsOnly => "Pick an item",
            Self::FluidsOnly => "Pick a fluid",
        }
    }
}

/// The chrome a picker window sits in: bottom right, above every panel, and
/// short enough to leave the panel that opened it readable behind it.
pub(crate) fn signal_picker_root() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(12.0),
            bottom: Val::Px(12.0),
            width: Val::Px(320.0),
            max_height: Val::Vh(50.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            padding: UiRect::all(Val::Px(10.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.035, 0.038, 0.040, 0.98)),
        GlobalZIndex(1200),
    )
}

/// Fills a picker window: a title, a button that clears the slot, and the grid.
///
/// `make_button` receives `None` for the clearing button and `Some(signal)` for
/// each cell, and returns the marker component that identifies the press to
/// whichever handler owns this picker.
pub(crate) fn spawn_signal_picker_contents<M: Bundle>(
    root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
    catalog: &PrototypeCatalog,
    filter: SignalFilter,
    clear_label: &str,
    mut make_button: impl FnMut(Option<SignalId>) -> M,
) {
    root.spawn((
        Text::new(filter.title().to_string()),
        TextFont::from_font_size(12.0),
        TextColor(Color::WHITE),
    ));
    root.spawn((
        Button,
        Node {
            width: Val::Px(110.0),
            height: Val::Px(20.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(BUTTON_BACKGROUND),
        make_button(None),
    ))
    .with_child((
        Text::new(clear_label.to_string()),
        TextFont::from_font_size(10.0),
        TextColor(Color::WHITE),
    ));

    let signals = SignalCatalog::from_catalog(catalog);
    root.spawn((
        Node {
            flex_grow: 1.0,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            overflow: Overflow::scroll_y(),
            scrollbar_width: 10.0,
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.022, 0.023, 0.75)),
        ScrollArea,
    ))
    .with_children(|viewport| {
        for (heading, section, entries) in [
            ("Items", SignalSection::Items, &signals.items),
            ("Fluids", SignalSection::Fluids, &signals.fluids),
            ("Signals", SignalSection::Virtuals, &signals.virtuals),
        ] {
            if entries.is_empty() || !filter.accepts(section) {
                continue;
            }
            viewport.spawn((
                Text::new(heading.to_string()),
                TextFont::from_font_size(10.0),
                TextColor(Color::srgb(0.70, 0.74, 0.80)),
            ));
            viewport
                .spawn((
                    Node {
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: Val::Px(3.0),
                        row_gap: Val::Px(3.0),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                ))
                .with_children(|grid| {
                    for &signal in entries {
                        grid.spawn((
                            Button,
                            Node {
                                width: Val::Px(CELL_WIDTH),
                                height: Val::Px(CELL_HEIGHT),
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                ..default()
                            },
                            BackgroundColor(BUTTON_BACKGROUND),
                            make_button(Some(signal)),
                        ))
                        .with_child((
                            Text::new(signal_short_label(catalog, signal)),
                            TextFont::from_font_size(9.0),
                            TextColor(Color::WHITE),
                            TextLayout::justify(Justify::Center),
                        ));
                    }
                });
        }
    });
}
