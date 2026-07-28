mod defense;
mod entities;
mod fluids;
mod foundations;
mod infrastructure;
mod items;
mod logistics;
mod power;
mod production;
mod rails;

use super::layers::VisualLayer;
use super::templates::VisualTemplate;
use bevy::prelude::*;
use items::{belt_item_layers, resource_layers};

pub(super) fn visual_layers(
    template: VisualTemplate,
    color: Color,
    size: Vec2,
) -> Vec<VisualLayer> {
    match template {
        VisualTemplate::Entity {
            kind,
            direction,
            connections,
            rail,
        } => entities::entity_layers(super::EntityVisualStyle {
            base_color: color,
            size,
            kind,
            direction,
            connections,
            rail,
        }),
        VisualTemplate::BeltItem => belt_item_layers(color, size),
        VisualTemplate::Resource => resource_layers(color, size),
    }
}

#[cfg(test)]
mod tests {
    use super::super::templates::ConnectionMask;
    use super::*;
    use crate::constants::TILE_SIZE;
    use factory_data::EntityKind;
    use factory_sim::Direction;

    fn entity_template(kind: EntityKind, connections: ConnectionMask) -> VisualTemplate {
        VisualTemplate::Entity {
            kind,
            direction: Direction::North,
            connections,
            rail: None,
        }
    }

    fn max_extent_x(layers: &[VisualLayer]) -> f32 {
        layers
            .iter()
            .map(|layer| layer.offset.x + layer.size.x * 0.5)
            .fold(f32::MIN, f32::max)
    }

    #[test]
    fn connected_pipe_grows_an_arm_reaching_the_tile_edge() {
        let size = Vec2::splat(TILE_SIZE * 0.92);
        let unconnected = visual_layers(
            entity_template(EntityKind::Pipe, ConnectionMask::EMPTY),
            Color::WHITE,
            size,
        );
        let east_connected = visual_layers(
            entity_template(
                EntityKind::Pipe,
                ConnectionMask::from_directions([false, true, false, false]),
            ),
            Color::WHITE,
            size,
        );

        assert!(max_extent_x(&east_connected) >= TILE_SIZE * 0.5 - 1e-4);
        assert!(max_extent_x(&unconnected) < TILE_SIZE * 0.5);
    }

    #[test]
    fn belt_connections_add_coupling_layers() {
        let size = Vec2::splat(TILE_SIZE * 0.92);
        let unconnected = visual_layers(
            entity_template(EntityKind::TransportBelt, ConnectionMask::EMPTY),
            Color::WHITE,
            size,
        );
        let joined = visual_layers(
            entity_template(
                EntityKind::TransportBelt,
                ConnectionMask::from_directions([true, false, true, false]),
            ),
            Color::WHITE,
            size,
        );

        assert_eq!(joined.len(), unconnected.len() + 2);
    }
}
