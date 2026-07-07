//! Shared lifecycle for snapshot-driven UI windows.
//!
//! Every panel follows the same dance: despawn when closed, spawn when
//! missing, despawn duplicates, and rebuild children only when the data they
//! were built from changed. [`sync_window`] implements that dance once; a
//! panel supplies its snapshot type, its root-node chrome, and a function
//! that spawns the contents.

use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;

/// Data a window's contents are derived from. One snapshot type per window:
/// the snapshot type is what identifies the window's root entity.
pub(crate) trait WindowSnapshot: PartialEq + Send + Sync + 'static {}

impl<S: PartialEq + Send + Sync + 'static> WindowSnapshot for S {}

/// Root component of a snapshot-driven window, holding the snapshot its
/// children were last built from.
#[derive(Component)]
pub(crate) struct WindowRoot<S: WindowSnapshot> {
    snapshot: S,
}

impl<S: WindowSnapshot> WindowRoot<S> {
    /// For roots spawned inside a larger hierarchy (see [`sync_contents`]);
    /// top-level windows are spawned by [`sync_window`] itself.
    pub(crate) fn new(snapshot: S) -> Self {
        Self { snapshot }
    }
}

pub(crate) type WindowRootQuery<'w, 's, S> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut WindowRoot<S>,
        Option<&'static Children>,
    ),
>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowSync {
    /// Window is closed; any roots were despawned.
    Closed,
    /// No root existed; the window was spawned with fresh contents.
    Spawned,
    /// The existing contents already match the snapshot.
    Unchanged,
    /// The snapshot changed; children were despawned and respawned.
    Rebuilt,
}

/// Drives one window through its lifecycle for this frame.
///
/// `inputs_changed` lets callers skip snapshot construction when none of the
/// inputs changed (pass `true` when the snapshot is cheap to build);
/// `make_snapshot` is only called when the window spawns or may rebuild.
pub(crate) fn sync_window<S: WindowSnapshot, B: Bundle>(
    commands: &mut Commands,
    roots: &mut WindowRootQuery<S>,
    open: bool,
    inputs_changed: bool,
    make_snapshot: impl FnOnce() -> S,
    root_bundle: impl FnOnce() -> B,
    spawn_contents: impl FnOnce(&mut ChildSpawnerCommands, &S),
) -> WindowSync {
    if !open {
        for (entity, _, _) in roots.iter() {
            commands.entity(entity).despawn();
        }
        return WindowSync::Closed;
    }

    let mut roots_iter = roots.iter_mut();
    let Some((root_entity, mut root, children)) = roots_iter.next() else {
        let snapshot = make_snapshot();
        let mut window = commands.spawn(root_bundle());
        window.with_children(|root| spawn_contents(root, &snapshot));
        window.insert(WindowRoot { snapshot });
        return WindowSync::Spawned;
    };
    for (duplicate, _, _) in roots_iter {
        commands.entity(duplicate).despawn();
    }

    if !inputs_changed {
        return WindowSync::Unchanged;
    }
    let snapshot = make_snapshot();
    if rebuild_contents(
        commands,
        root_entity,
        &mut root,
        children,
        snapshot,
        spawn_contents,
    ) {
        WindowSync::Rebuilt
    } else {
        WindowSync::Unchanged
    }
}

/// Like [`sync_window`] for a root that lives inside another window's
/// hierarchy: the outer window spawns and despawns it (via
/// [`WindowRoot::new`]), so this only compares and rebuilds.
pub(crate) fn sync_contents<S: WindowSnapshot>(
    commands: &mut Commands,
    roots: &mut WindowRootQuery<S>,
    snapshot: S,
    spawn_contents: impl FnOnce(&mut ChildSpawnerCommands, &S),
) {
    let mut roots_iter = roots.iter_mut();
    let Some((root_entity, mut root, children)) = roots_iter.next() else {
        return;
    };
    for (duplicate, _, _) in roots_iter {
        commands.entity(duplicate).despawn();
    }
    rebuild_contents(
        commands,
        root_entity,
        &mut root,
        children,
        snapshot,
        spawn_contents,
    );
}

fn rebuild_contents<S: WindowSnapshot>(
    commands: &mut Commands,
    root_entity: Entity,
    root: &mut WindowRoot<S>,
    children: Option<&Children>,
    snapshot: S,
    spawn_contents: impl FnOnce(&mut ChildSpawnerCommands, &S),
) -> bool {
    if root.snapshot == snapshot {
        return false;
    }
    if let Some(children) = children {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }
    commands
        .entity(root_entity)
        .with_children(|root| spawn_contents(root, &snapshot));
    root.snapshot = snapshot;
    true
}
