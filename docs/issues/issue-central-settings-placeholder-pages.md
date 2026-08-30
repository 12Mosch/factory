# Issue: Implement missing central Settings pages

## Summary

The central Settings menu should expose placeholder pages now, while the
underlying features are tracked for later implementation.

## Scope

Create and later implement these pages:

- **Display** — fullscreen/windowed mode, resolution, VSync, graphics quality,
  and UI scale.
- **Controls** — inspect and rebind keyboard/mouse actions, reset bindings,
  and show current shortcuts.
- **Accessibility** — text/UI scale, colorblind support, screen-shake toggle,
  and other accessibility options as they become available.

## Acceptance criteria

- Each page is reachable from the central Settings menu.
- Until implemented, each page clearly says that the feature is unavailable
  and does not present controls that appear functional.
- Once implemented, settings are persisted independently from save-game data.
- Automated tests cover navigation and the settings' effect on the relevant
  Bevy resources.
