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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct TestSnapshot(u32);

    #[derive(Component)]
    struct TestRootMarker;

    #[derive(Component)]
    struct TestChild(u32);

    #[derive(Resource)]
    struct TestWindowState {
        open: bool,
        inputs_changed: bool,
        snapshot: u32,
        result: Option<WindowSync>,
    }

    #[derive(Resource)]
    struct TestContentsState {
        snapshot: u32,
    }

    #[test]
    fn sync_window_drives_closed_spawned_unchanged_and_rebuilt_states() {
        let mut app = App::new();
        app.insert_resource(TestWindowState {
            open: false,
            inputs_changed: true,
            snapshot: 1,
            result: None,
        })
        .add_systems(Update, sync_test_window);

        app.update();
        assert_eq!(window_result(&app), WindowSync::Closed);
        assert_eq!(root_count(&mut app), 0);
        assert_eq!(test_child_values(&mut app), Vec::<u32>::new());

        {
            let mut state = app.world_mut().resource_mut::<TestWindowState>();
            state.open = true;
            state.inputs_changed = true;
            state.snapshot = 1;
        }
        app.update();
        assert_eq!(window_result(&app), WindowSync::Spawned);
        assert_eq!(root_count(&mut app), 1);
        assert_eq!(root_child_counts(&mut app), vec![1]);
        assert_eq!(test_child_values(&mut app), vec![1]);

        app.world_mut()
            .spawn((WindowRoot::new(TestSnapshot(99)), TestRootMarker))
            .with_child((TestChild(99),));
        assert_eq!(root_count(&mut app), 2);
        assert_eq!(test_child_values(&mut app), vec![1, 99]);

        {
            let mut state = app.world_mut().resource_mut::<TestWindowState>();
            state.inputs_changed = false;
            state.snapshot = 2;
        }
        app.update();
        assert_eq!(window_result(&app), WindowSync::Unchanged);
        assert_eq!(root_count(&mut app), 1);
        assert_eq!(root_child_counts(&mut app), vec![1]);
        assert_eq!(test_child_values(&mut app), vec![1]);

        app.world_mut()
            .resource_mut::<TestWindowState>()
            .inputs_changed = true;
        app.update();
        assert_eq!(window_result(&app), WindowSync::Rebuilt);
        assert_eq!(root_count(&mut app), 1);
        assert_eq!(root_child_counts(&mut app), vec![1]);
        assert_eq!(test_child_values(&mut app), vec![2]);

        app.world_mut().resource_mut::<TestWindowState>().open = false;
        app.update();
        assert_eq!(window_result(&app), WindowSync::Closed);
        assert_eq!(root_count(&mut app), 0);
        assert_eq!(test_child_values(&mut app), Vec::<u32>::new());
    }

    #[test]
    fn sync_contents_rebuilds_existing_root_and_removes_duplicates() {
        let mut app = App::new();
        app.insert_resource(TestContentsState { snapshot: 1 })
            .add_systems(Update, sync_test_contents);

        app.world_mut()
            .spawn((WindowRoot::new(TestSnapshot(1)), TestRootMarker))
            .with_child((TestChild(1),));
        app.update();
        assert_eq!(root_count(&mut app), 1);
        assert_eq!(root_child_counts(&mut app), vec![1]);
        assert_eq!(test_child_values(&mut app), vec![1]);

        app.world_mut()
            .spawn((WindowRoot::new(TestSnapshot(99)), TestRootMarker))
            .with_child((TestChild(99),));
        app.world_mut().resource_mut::<TestContentsState>().snapshot = 2;

        app.update();
        assert_eq!(root_count(&mut app), 1);
        assert_eq!(root_child_counts(&mut app), vec![1]);
        assert_eq!(test_child_values(&mut app), vec![2]);
    }

    fn sync_test_window(
        mut commands: Commands,
        mut state: ResMut<TestWindowState>,
        mut roots: WindowRootQuery<TestSnapshot>,
    ) {
        let open = state.open;
        let inputs_changed = state.inputs_changed;
        let snapshot = state.snapshot;
        let result = sync_window(
            &mut commands,
            &mut roots,
            open,
            inputs_changed,
            || TestSnapshot(snapshot),
            || (TestRootMarker,),
            spawn_test_child,
        );
        state.result = Some(result);
    }

    fn sync_test_contents(
        mut commands: Commands,
        state: Res<TestContentsState>,
        mut roots: WindowRootQuery<TestSnapshot>,
    ) {
        sync_contents(
            &mut commands,
            &mut roots,
            TestSnapshot(state.snapshot),
            spawn_test_child,
        );
    }

    fn spawn_test_child(
        root: &mut bevy::ecs::hierarchy::ChildSpawnerCommands,
        snapshot: &TestSnapshot,
    ) {
        root.spawn((TestChild(snapshot.0),));
    }

    fn window_result(app: &App) -> WindowSync {
        app.world()
            .resource::<TestWindowState>()
            .result
            .expect("window system should store a sync result")
    }

    fn root_count(app: &mut App) -> usize {
        let world = app.world_mut();
        let mut roots = world.query::<&WindowRoot<TestSnapshot>>();
        roots.iter(world).count()
    }

    fn root_child_counts(app: &mut App) -> Vec<usize> {
        let world = app.world_mut();
        let mut roots = world.query::<(Option<&Children>, &WindowRoot<TestSnapshot>)>();
        let mut counts = roots
            .iter(world)
            .map(|(children, _)| children.map_or(0, Children::len))
            .collect::<Vec<_>>();
        counts.sort_unstable();
        counts
    }

    fn test_child_values(app: &mut App) -> Vec<u32> {
        let world = app.world_mut();
        let mut children = world.query::<&TestChild>();
        let mut values = children
            .iter(world)
            .map(|child| child.0)
            .collect::<Vec<_>>();
        values.sort_unstable();
        values
    }
}
