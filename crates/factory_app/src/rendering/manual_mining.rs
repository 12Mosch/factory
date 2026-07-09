use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::constants::{MANUAL_MINING_BAR_HEIGHT, MANUAL_MINING_BAR_WIDTH, TILE_SIZE};
use crate::interaction::cursor::{CursorCameraFilter, cursor_tile_from_window};
use crate::rendering::transforms::{manual_mining_bar_translation, tile_translation};
use crate::resources::SimResource;

#[derive(Component)]
pub(crate) struct CursorTileHighlight;

#[derive(Component)]
pub(crate) struct ManualMiningProgressBarBackground;

#[derive(Component)]
pub(crate) struct ManualMiningProgressBarFill;

pub(crate) type ManualMiningProgressBarBackgroundFilter = (
    With<ManualMiningProgressBarBackground>,
    Without<ManualMiningProgressBarFill>,
);
pub(crate) type ManualMiningProgressBarBackgroundQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut Transform, &'static mut Visibility),
    ManualMiningProgressBarBackgroundFilter,
>;
pub(crate) type ManualMiningProgressBarFillQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Transform,
        &'static mut Visibility,
        &'static mut Sprite,
    ),
    With<ManualMiningProgressBarFill>,
>;

pub(crate) fn spawn_cursor_tile_highlight(mut commands: Commands) {
    commands.spawn((
        Sprite::from_color(Color::srgba(1.0, 1.0, 1.0, 0.28), Vec2::splat(TILE_SIZE)),
        Transform::from_translation(Vec3::new(0.0, 0.0, 3.0)),
        Visibility::Hidden,
        CursorTileHighlight,
    ));
}

pub(crate) fn spawn_manual_mining_progress_bar(mut commands: Commands) {
    commands.spawn((
        Sprite::from_color(
            Color::srgba(0.02, 0.02, 0.02, 0.82),
            Vec2::new(MANUAL_MINING_BAR_WIDTH, MANUAL_MINING_BAR_HEIGHT),
        ),
        Transform::from_translation(Vec3::new(0.0, 0.0, 5.0)),
        Visibility::Hidden,
        ManualMiningProgressBarBackground,
    ));
    commands.spawn((
        Sprite::from_color(
            Color::srgb(0.34, 0.82, 0.38),
            Vec2::new(0.0, MANUAL_MINING_BAR_HEIGHT),
        ),
        Transform::from_translation(Vec3::new(0.0, 0.0, 5.1)),
        Visibility::Hidden,
        ManualMiningProgressBarFill,
    ));
}

pub(crate) fn update_cursor_tile_highlight(
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), CursorCameraFilter>,
    mut highlights: Query<(&mut Transform, &mut Visibility), With<CursorTileHighlight>>,
) {
    let cursor_tile = cursor_tile_from_window(&windows, &cameras);

    for (mut transform, mut visibility) in &mut highlights {
        if let Some((x, y)) = cursor_tile {
            transform.translation = tile_translation(x, y, transform.translation.z);
            *visibility = Visibility::Visible;
        } else {
            *visibility = Visibility::Hidden;
        }
    }
}

pub(crate) fn update_manual_mining_progress_bar(
    sim: Res<SimResource>,
    mut backgrounds: ManualMiningProgressBarBackgroundQuery,
    mut fills: ManualMiningProgressBarFillQuery,
) {
    let progress = sim.read().manual_mining_progress();

    for (mut transform, mut visibility) in &mut backgrounds {
        if let Some(progress) = progress {
            transform.translation =
                manual_mining_bar_translation(progress.target.x, progress.target.y, 5.0);
            *visibility = Visibility::Visible;
        } else {
            *visibility = Visibility::Hidden;
        }
    }

    for (mut transform, mut visibility, mut sprite) in &mut fills {
        if let Some(progress) = progress {
            let fill_ratio =
                (progress.progress_ticks as f32 / progress.required_ticks as f32).clamp(0.0, 1.0);
            let fill_width = MANUAL_MINING_BAR_WIDTH * fill_ratio;
            let mut translation =
                manual_mining_bar_translation(progress.target.x, progress.target.y, 5.1);
            translation.x += (fill_width - MANUAL_MINING_BAR_WIDTH) * 0.5;
            transform.translation = translation;
            sprite.custom_size = Some(Vec2::new(fill_width, MANUAL_MINING_BAR_HEIGHT));
            *visibility = Visibility::Visible;
        } else {
            *visibility = Visibility::Hidden;
        }
    }
}
