# Central Settings Menu Plan

## Goal

Provide one discoverable Settings window that groups existing options and gives
players clear placeholder pages for settings that are not implemented yet.

## Small implementation plan

1. Add a visible **Settings** button to the existing pause/save menu and retain
   a keyboard shortcut as an alternative entry point.
2. Add a `SettingsWindowState` resource with `open`, `active_tab`, and pending
   values/dirty state. Make the central window own tab navigation and closing.
3. Move the existing Audio Settings controls into an **Audio** tab and the
   existing enemy preset controls into a **Gameplay** tab. Keep `O` and `N` as
   compatibility shortcuts that open those tabs.
4. Add placeholder tabs for **Display**, **Controls**, and **Accessibility**.
   Each should explain that the feature is not available yet and link its
   follow-up issue in the source comment/documentation.
5. Add Apply, Reset, and Back behavior. Existing audio changes may be applied
   immediately; future multi-value settings should use pending values and
   commit on Apply.
6. Add UI/system tests covering opening from the pause menu, tab switching,
   placeholder rendering, Escape behavior, and the existing audio/gameplay
   actions.

## Follow-up issues

See [issue-central-settings-placeholder-pages.md](issues/issue-central-settings-placeholder-pages.md)
for the missing Display, Controls, and Accessibility implementations.
