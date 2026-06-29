use bevy::prelude::*;

use crate::resources::{MapTextureCache, MapViewState};

#[derive(Component)]
pub(crate) struct MinimapRoot;

#[derive(Component)]
pub(crate) struct FullMapRoot;

pub(crate) fn sync_minimap(
    mut commands: Commands,
    cache: Res<MapTextureCache>,
    roots: Query<Entity, With<MinimapRoot>>,
) {
    let Some(handle) = cache.handle.as_ref() else {
        return;
    };
    let mut roots_iter = roots.iter();
    if roots_iter.next().is_some() {
        for duplicate in roots_iter {
            commands.entity(duplicate).despawn();
        }
        return;
    }

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(14.0),
                top: Val::Px(14.0),
                width: Val::Px(184.0),
                height: Val::Px(184.0),
                padding: UiRect::all(Val::Px(4.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.025, 0.027, 0.88)),
            BorderColor::all(Color::srgba(0.36, 0.38, 0.34, 0.82)),
            GlobalZIndex(1800),
            MinimapRoot,
        ))
        .with_child((
            ImageNode::new(handle.clone()),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
        ));
}

pub(crate) fn sync_full_map_view(
    mut commands: Commands,
    cache: Res<MapTextureCache>,
    state: Res<MapViewState>,
    roots: Query<Entity, With<FullMapRoot>>,
) {
    if !state.open {
        for entity in &roots {
            commands.entity(entity).despawn();
        }
        return;
    }

    let Some(handle) = cache.handle.as_ref() else {
        return;
    };
    let mut roots_iter = roots.iter();
    let Some(_root) = roots_iter.next() else {
        spawn_full_map(&mut commands, handle.clone());
        return;
    };
    for duplicate in roots_iter {
        commands.entity(duplicate).despawn();
    }
}

fn spawn_full_map(commands: &mut Commands, handle: Handle<Image>) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::all(Val::Px(28.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.015, 0.017, 0.018, 0.96)),
            GlobalZIndex(2200),
            FullMapRoot,
        ))
        .with_child((
            ImageNode::new(handle),
            Node {
                width: Val::Percent(84.0),
                height: Val::Percent(84.0),
                max_width: Val::Px(980.0),
                max_height: Val::Px(980.0),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(Color::srgba(0.42, 0.43, 0.39, 0.9)),
        ));
}
