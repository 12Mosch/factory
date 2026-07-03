use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;
use factory_data::EntityKind;
use factory_sim::Direction;

use crate::constants::TILE_SIZE;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EntityVisualStyle {
    pub(crate) base_color: Color,
    pub(crate) size: Vec2,
    pub(crate) kind: EntityKind,
    pub(crate) direction: Direction,
}

pub(crate) fn spawn_entity_visual<B: Bundle>(
    commands: &mut Commands,
    style: EntityVisualStyle,
    translation: Vec3,
    marker: B,
) -> Entity {
    commands
        .spawn((
            Sprite::from_color(style.base_color, style.size),
            Transform::from_translation(translation),
            marker,
        ))
        .with_children(|parent| spawn_entity_layers(parent, style))
        .id()
}

pub(crate) fn spawn_belt_item_visual<B: Bundle>(
    commands: &mut Commands,
    color: Color,
    size: Vec2,
    translation: Vec3,
    marker: B,
) -> Entity {
    commands
        .spawn((
            Sprite::from_color(color, size),
            Transform::from_translation(translation),
            marker,
        ))
        .with_children(|parent| {
            spawn_layer(
                parent,
                Vec2::new(size.x * 1.12, size.y * 1.12),
                Vec2::new(size.x * 0.10, -size.y * 0.10),
                -0.10,
                Color::srgba(0.02, 0.018, 0.014, 0.42),
            );
            spawn_layer(
                parent,
                Vec2::new(size.x * 0.72, size.y * 0.22),
                Vec2::new(0.0, size.y * 0.18),
                0.10,
                Color::srgba(1.0, 0.96, 0.78, 0.30),
            );
            spawn_layer(
                parent,
                Vec2::new(size.x * 0.22, size.y * 0.70),
                Vec2::ZERO,
                0.12,
                Color::srgba(0.02, 0.02, 0.02, 0.24),
            );
        })
        .id()
}

pub(crate) fn spawn_resource_visual<B: Bundle>(
    commands: &mut Commands,
    color: Color,
    size: Vec2,
    transform: Transform,
    marker: B,
) -> Entity {
    commands
        .spawn((Sprite::from_color(color, size), transform, marker))
        .with_children(|parent| {
            spawn_layer(
                parent,
                Vec2::new(size.x * 1.18, size.y * 0.58),
                Vec2::new(size.x * 0.08, -size.y * 0.18),
                -0.08,
                Color::srgba(0.02, 0.018, 0.014, 0.38),
            );
            spawn_layer(
                parent,
                Vec2::new(size.x * 0.52, size.y * 0.22),
                Vec2::new(-size.x * 0.12, size.y * 0.16),
                0.08,
                Color::srgba(1.0, 0.94, 0.74, 0.25),
            );
        })
        .id()
}

fn spawn_entity_layers(parent: &mut ChildSpawnerCommands, style: EntityVisualStyle) {
    spawn_shadow(parent, style.size);
    spawn_entity_outline(parent, style.size);

    match style.kind {
        EntityKind::TransportBelt => spawn_transport_belt_layers(parent, style),
        EntityKind::Splitter => spawn_splitter_layers(parent, style),
        EntityKind::Chest => spawn_chest_layers(parent, style),
        EntityKind::MiningDrill => spawn_drill_layers(parent, style),
        EntityKind::Furnace => spawn_furnace_layers(parent, style),
        EntityKind::AssemblingMachine => spawn_assembler_layers(parent, style),
        EntityKind::Lab => spawn_lab_layers(parent, style),
        EntityKind::Inserter => spawn_inserter_layers(parent, style),
        EntityKind::ElectricPole => spawn_electric_pole_layers(parent, style),
        EntityKind::SteamEngine => spawn_steam_engine_layers(parent, style),
        EntityKind::Boiler => spawn_boiler_layers(parent, style),
        EntityKind::OffshorePump => spawn_offshore_pump_layers(parent, style),
        EntityKind::Pipe => spawn_pipe_layers(parent, style),
        EntityKind::StorageTank => spawn_storage_tank_layers(parent, style),
        EntityKind::ResourcePatch => {}
    }
}

fn spawn_transport_belt_layers(parent: &mut ChildSpawnerCommands, style: EntityVisualStyle) {
    let horizontal = is_horizontal(style.direction);
    let rail = if horizontal {
        Vec2::new(style.size.x * 0.88, style.size.y * 0.13)
    } else {
        Vec2::new(style.size.x * 0.13, style.size.y * 0.88)
    };
    let lane_offset = if horizontal {
        Vec2::new(0.0, style.size.y * 0.23)
    } else {
        Vec2::new(style.size.x * 0.23, 0.0)
    };
    let seam = if horizontal {
        Vec2::new(style.size.x * 0.84, style.size.y * 0.05)
    } else {
        Vec2::new(style.size.x * 0.05, style.size.y * 0.84)
    };

    spawn_layer(
        parent,
        rail,
        lane_offset,
        0.08,
        Color::srgba(0.09, 0.075, 0.055, 0.64),
    );
    spawn_layer(
        parent,
        rail,
        -lane_offset,
        0.08,
        Color::srgba(0.09, 0.075, 0.055, 0.64),
    );
    spawn_layer(
        parent,
        seam,
        Vec2::ZERO,
        0.09,
        Color::srgba(1.0, 0.88, 0.48, 0.36),
    );
}

fn spawn_splitter_layers(parent: &mut ChildSpawnerCommands, style: EntityVisualStyle) {
    spawn_transport_belt_layers(parent, style);
    let horizontal = is_horizontal(style.direction);
    let divider = if horizontal {
        Vec2::new(style.size.x * 0.12, style.size.y * 0.82)
    } else {
        Vec2::new(style.size.x * 0.82, style.size.y * 0.12)
    };
    let port = Vec2::splat(TILE_SIZE * 0.22);
    let offset = if horizontal {
        Vec2::new(style.size.x * 0.30, 0.0)
    } else {
        Vec2::new(0.0, style.size.y * 0.30)
    };

    spawn_layer(
        parent,
        divider,
        Vec2::ZERO,
        0.14,
        Color::srgba(0.10, 0.08, 0.06, 0.70),
    );
    spawn_layer(
        parent,
        port,
        offset,
        0.16,
        Color::srgba(0.95, 0.90, 0.68, 0.48),
    );
    spawn_layer(
        parent,
        port,
        -offset,
        0.16,
        Color::srgba(0.95, 0.90, 0.68, 0.48),
    );
}

fn spawn_chest_layers(parent: &mut ChildSpawnerCommands, style: EntityVisualStyle) {
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.82, style.size.y * 0.18),
        Vec2::new(0.0, style.size.y * 0.18),
        0.10,
        tinted(style.base_color, 0.24),
    );
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.18, style.size.y * 0.30),
        Vec2::ZERO,
        0.12,
        Color::srgba(0.95, 0.74, 0.38, 0.72),
    );
}

fn spawn_drill_layers(parent: &mut ChildSpawnerCommands, style: EntityVisualStyle) {
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.74, style.size.y * 0.24),
        Vec2::new(0.0, style.size.y * 0.18),
        0.10,
        Color::srgba(0.13, 0.15, 0.14, 0.72),
    );
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.22, style.size.y * 0.70),
        direction_offset(style.direction, style.size * 0.14),
        0.12,
        Color::srgba(0.82, 0.68, 0.38, 0.84),
    );
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.40, style.size.y * 0.16),
        -direction_offset(style.direction, style.size * 0.20),
        0.12,
        Color::srgba(0.10, 0.10, 0.09, 0.66),
    );
}

fn spawn_furnace_layers(parent: &mut ChildSpawnerCommands, style: EntityVisualStyle) {
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.46, style.size.y * 0.34),
        Vec2::new(0.0, -style.size.y * 0.10),
        0.10,
        Color::srgba(0.95, 0.36, 0.12, 0.72),
    );
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.18, style.size.y * 0.58),
        Vec2::new(style.size.x * 0.26, style.size.y * 0.05),
        0.11,
        Color::srgba(0.13, 0.12, 0.11, 0.72),
    );
}

fn spawn_assembler_layers(parent: &mut ChildSpawnerCommands, style: EntityVisualStyle) {
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.66, style.size.y * 0.12),
        Vec2::ZERO,
        0.10,
        Color::srgba(0.74, 0.88, 0.92, 0.50),
    );
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.12, style.size.y * 0.66),
        Vec2::ZERO,
        0.11,
        Color::srgba(0.74, 0.88, 0.92, 0.50),
    );
    spawn_layer(
        parent,
        Vec2::splat(TILE_SIZE * 0.26),
        Vec2::ZERO,
        0.12,
        Color::srgba(0.09, 0.12, 0.13, 0.68),
    );
}

fn spawn_lab_layers(parent: &mut ChildSpawnerCommands, style: EntityVisualStyle) {
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.62, style.size.y * 0.38),
        Vec2::new(0.0, style.size.y * 0.04),
        0.10,
        Color::srgba(0.42, 0.86, 0.78, 0.52),
    );
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.82, style.size.y * 0.10),
        Vec2::new(0.0, -style.size.y * 0.28),
        0.11,
        Color::srgba(0.92, 0.80, 0.44, 0.42),
    );
}

fn spawn_inserter_layers(parent: &mut ChildSpawnerCommands, style: EntityVisualStyle) {
    let along = direction_vec(style.direction);
    let horizontal = is_horizontal(style.direction);
    let arm = if horizontal {
        Vec2::new(style.size.x * 0.64, style.size.y * 0.16)
    } else {
        Vec2::new(style.size.x * 0.16, style.size.y * 0.64)
    };
    let hand = Vec2::splat(TILE_SIZE * 0.22);

    spawn_layer(
        parent,
        Vec2::splat(TILE_SIZE * 0.44),
        Vec2::ZERO,
        0.08,
        Color::srgba(0.12, 0.10, 0.07, 0.68),
    );
    spawn_layer(
        parent,
        arm,
        along * TILE_SIZE * 0.10,
        0.12,
        Color::srgba(0.88, 0.72, 0.32, 0.86),
    );
    spawn_layer(
        parent,
        hand,
        along * TILE_SIZE * 0.34,
        0.14,
        Color::srgba(0.12, 0.10, 0.08, 0.76),
    );
}

fn spawn_electric_pole_layers(parent: &mut ChildSpawnerCommands, style: EntityVisualStyle) {
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.20, style.size.y * 0.82),
        Vec2::ZERO,
        0.10,
        Color::srgba(0.18, 0.12, 0.08, 0.82),
    );
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.78, style.size.y * 0.14),
        Vec2::new(0.0, style.size.y * 0.20),
        0.12,
        Color::srgba(0.96, 0.82, 0.42, 0.72),
    );
}

fn spawn_steam_engine_layers(parent: &mut ChildSpawnerCommands, style: EntityVisualStyle) {
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.78, style.size.y * 0.24),
        Vec2::ZERO,
        0.10,
        Color::srgba(0.70, 0.86, 0.90, 0.40),
    );
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.18, style.size.y * 0.70),
        Vec2::new(style.size.x * 0.28, 0.0),
        0.12,
        Color::srgba(0.12, 0.16, 0.17, 0.60),
    );
}

fn spawn_boiler_layers(parent: &mut ChildSpawnerCommands, style: EntityVisualStyle) {
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.72, style.size.y * 0.26),
        Vec2::new(0.0, -style.size.y * 0.12),
        0.10,
        Color::srgba(0.96, 0.48, 0.16, 0.60),
    );
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.68, style.size.y * 0.16),
        Vec2::new(0.0, style.size.y * 0.22),
        0.11,
        Color::srgba(0.20, 0.22, 0.22, 0.70),
    );
}

fn spawn_offshore_pump_layers(parent: &mut ChildSpawnerCommands, style: EntityVisualStyle) {
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.62, style.size.y * 0.32),
        Vec2::new(0.0, -style.size.y * 0.12),
        0.10,
        Color::srgba(0.48, 0.82, 0.94, 0.58),
    );
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.20, style.size.y * 0.72),
        Vec2::new(style.size.x * 0.24, 0.0),
        0.11,
        Color::srgba(0.08, 0.16, 0.20, 0.58),
    );
}

fn spawn_pipe_layers(parent: &mut ChildSpawnerCommands, style: EntityVisualStyle) {
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.82, style.size.y * 0.18),
        Vec2::ZERO,
        0.10,
        Color::srgba(0.78, 0.90, 0.94, 0.40),
    );
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.18, style.size.y * 0.82),
        Vec2::ZERO,
        0.11,
        Color::srgba(0.20, 0.24, 0.25, 0.38),
    );
}

fn spawn_storage_tank_layers(parent: &mut ChildSpawnerCommands, style: EntityVisualStyle) {
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.74, style.size.y * 0.18),
        Vec2::new(0.0, style.size.y * 0.20),
        0.10,
        Color::srgba(0.78, 0.90, 0.92, 0.46),
    );
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.74, style.size.y * 0.18),
        Vec2::new(0.0, -style.size.y * 0.20),
        0.10,
        Color::srgba(0.18, 0.23, 0.24, 0.42),
    );
    spawn_layer(
        parent,
        Vec2::new(style.size.x * 0.18, style.size.y * 0.76),
        Vec2::new(style.size.x * 0.25, 0.0),
        0.11,
        Color::srgba(0.12, 0.16, 0.17, 0.42),
    );
}

fn spawn_shadow(parent: &mut ChildSpawnerCommands, size: Vec2) {
    spawn_layer(
        parent,
        Vec2::new(size.x * 1.04, size.y * 1.04),
        Vec2::new(TILE_SIZE * 0.06, -TILE_SIZE * 0.06),
        -0.16,
        Color::srgba(0.015, 0.012, 0.010, 0.46),
    );
}

fn spawn_entity_outline(parent: &mut ChildSpawnerCommands, size: Vec2) {
    spawn_layer(
        parent,
        Vec2::new(size.x * 1.02, size.y * 1.02),
        Vec2::ZERO,
        -0.08,
        Color::srgba(0.035, 0.030, 0.026, 0.56),
    );
    spawn_layer(
        parent,
        Vec2::new(size.x * 0.78, size.y * 0.10),
        Vec2::new(0.0, size.y * 0.36),
        0.08,
        Color::srgba(1.0, 0.95, 0.72, 0.18),
    );
}

fn spawn_layer(parent: &mut ChildSpawnerCommands, size: Vec2, offset: Vec2, z: f32, color: Color) {
    parent.spawn((
        Sprite::from_color(color, size),
        Transform::from_translation(Vec3::new(offset.x, offset.y, z)),
    ));
}

fn is_horizontal(direction: Direction) -> bool {
    matches!(direction, Direction::East | Direction::West)
}

fn direction_vec(direction: Direction) -> Vec2 {
    match direction {
        Direction::North => Vec2::Y,
        Direction::East => Vec2::X,
        Direction::South => Vec2::NEG_Y,
        Direction::West => Vec2::NEG_X,
    }
}

fn direction_offset(direction: Direction, size: Vec2) -> Vec2 {
    let along = direction_vec(direction);
    Vec2::new(along.x * size.x, along.y * size.y)
}

fn tinted(color: Color, amount: f32) -> Color {
    let color = color.to_srgba();
    Color::srgba(
        color.red + (1.0 - color.red) * amount,
        color.green + (1.0 - color.green) * amount,
        color.blue + (1.0 - color.blue) * amount,
        color.alpha,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_horizontal_identifies_east_and_west() {
        assert!(!is_horizontal(Direction::North));
        assert!(is_horizontal(Direction::East));
        assert!(!is_horizontal(Direction::South));
        assert!(is_horizontal(Direction::West));
    }

    #[test]
    fn direction_vec_maps_cardinal_axes() {
        assert_eq!(direction_vec(Direction::North), Vec2::Y);
        assert_eq!(direction_vec(Direction::East), Vec2::X);
        assert_eq!(direction_vec(Direction::South), Vec2::NEG_Y);
        assert_eq!(direction_vec(Direction::West), Vec2::NEG_X);
    }

    #[test]
    fn direction_offset_applies_axis_specific_size() {
        let size = Vec2::new(3.0, 5.0);

        assert_eq!(
            direction_offset(Direction::North, size),
            Vec2::new(0.0, 5.0)
        );
        assert_eq!(direction_offset(Direction::East, size), Vec2::new(3.0, 0.0));
        assert_eq!(
            direction_offset(Direction::South, size),
            Vec2::new(0.0, -5.0)
        );
        assert_eq!(
            direction_offset(Direction::West, size),
            Vec2::new(-3.0, 0.0)
        );
    }

    #[test]
    fn tinted_moves_color_toward_white_and_preserves_alpha() {
        let original = Color::srgba(0.20, 0.40, 0.60, 0.70);
        let tinted = tinted(original, 0.25).to_srgba();

        assert_close(tinted.red, 0.40);
        assert_close(tinted.green, 0.55);
        assert_close(tinted.blue, 0.70);
        assert_close(tinted.alpha, 0.70);
    }

    #[test]
    fn tinted_handles_identity_and_full_white_edges() {
        let original = Color::srgba(0.20, 0.40, 0.60, 0.70);
        let unchanged = tinted(original, 0.0).to_srgba();
        let white = tinted(original, 1.0).to_srgba();

        assert_close(unchanged.red, 0.20);
        assert_close(unchanged.green, 0.40);
        assert_close(unchanged.blue, 0.60);
        assert_close(unchanged.alpha, 0.70);
        assert_close(white.red, 1.0);
        assert_close(white.green, 1.0);
        assert_close(white.blue, 1.0);
        assert_close(white.alpha, 0.70);
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= f32::EPSILON,
            "expected {actual} to equal {expected}"
        );
    }
}
