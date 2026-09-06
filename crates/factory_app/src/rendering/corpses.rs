use crate::constants::TILE_SIZE;
use crate::rendering::transforms::tile_translation;
use crate::resources::SimResource;
use bevy::prelude::*;
use std::collections::BTreeSet;

pub(crate) const CORPSE_COLOR: Color = Color::srgb(0.85, 0.25, 0.62);

#[derive(Component)]
pub(crate) struct CorpseSprite(u64);

pub(crate) fn sync_corpses(
    mut commands: Commands,
    sim: Res<SimResource>,
    mut markers: Query<(Entity, &CorpseSprite, &mut Transform)>,
) {
    let simulation = sim.read();
    let mut retained = BTreeSet::new();
    for (entity, marker, mut transform) in &mut markers {
        if let Some(corpse) = simulation.corpse(marker.0) {
            let (x, y) = corpse.tile_position();
            transform.translation = tile_translation(x, y, 4.5);
            retained.insert(marker.0);
        } else {
            commands.entity(entity).despawn();
        }
    }
    for corpse in simulation
        .corpses()
        .filter(|corpse| !retained.contains(&corpse.id()))
    {
        let (x, y) = corpse.tile_position();
        commands.spawn((
            CorpseSprite(corpse.id()),
            Sprite::from_color(CORPSE_COLOR, Vec2::new(TILE_SIZE * 0.8, TILE_SIZE * 0.35)),
            Transform::from_translation(tile_translation(x, y, 4.5))
                .with_rotation(Quat::from_rotation_z(0.45)),
        ));
    }
}

pub(crate) fn clear_corpses(mut commands: Commands, markers: Query<Entity, With<CorpseSprite>>) {
    for entity in &markers {
        commands.entity(entity).despawn();
    }
}
