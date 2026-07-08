use bevy::prelude::*;
use factory_sim::Direction;

pub(super) fn slot_background_color(
    interaction: Interaction,
    selected: bool,
    available: bool,
) -> Color {
    if selected {
        Color::srgba(0.34, 0.26, 0.12, 0.98)
    } else if !available {
        Color::srgba(0.075, 0.075, 0.075, 0.86)
    } else {
        match interaction {
            Interaction::Pressed => Color::srgba(0.24, 0.22, 0.18, 0.98),
            Interaction::Hovered => Color::srgba(0.19, 0.18, 0.16, 0.98),
            Interaction::None => Color::srgba(0.13, 0.13, 0.13, 0.95),
        }
    }
}

pub(super) fn action_background_color(interaction: Interaction, active: bool) -> Color {
    if !active {
        return Color::srgba(0.09, 0.09, 0.09, 0.82);
    }

    match interaction {
        Interaction::Pressed => Color::srgba(0.27, 0.23, 0.16, 0.98),
        Interaction::Hovered => Color::srgba(0.21, 0.19, 0.16, 0.98),
        Interaction::None => Color::srgba(0.15, 0.15, 0.15, 0.95),
    }
}

pub(super) fn action_border_color(active: bool) -> Color {
    if active {
        Color::srgba(0.66, 0.55, 0.35, 0.85)
    } else {
        Color::srgba(0.33, 0.32, 0.30, 0.60)
    }
}

pub(super) fn direction_abbreviation(direction: Direction) -> &'static str {
    match direction {
        Direction::North => "N",
        Direction::East => "E",
        Direction::South => "S",
        Direction::West => "W",
    }
}
