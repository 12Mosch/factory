# Desktop display settings

Settings → Display offers windowed and borderless fullscreen on the current
monitor, VSync, and frame limits of 30, 60, 90, 120, 144 FPS or Unlimited.
The default is windowed, automatic window size, VSync on and a 60 FPS limit.
The limit paces application frames using wall time; fixed simulation scheduling
and catch-up policy remain unchanged. VSync can impose a lower effective limit.
Bevy's AutoVsync / AutoNoVsync presentation modes fall back to a supported mode
when the GPU or compositor cannot honor the requested setting.

Window sizes are logical client dimensions, so OS scaling affects physical pixel
dimensions. Presets are offered only when they fit inside 90% of the current
monitor's logical size, leaving room for decorations and desktop panels. Auto fit
targets 1280×720 within that bound. If monitor information is unavailable, it uses
960×540. Moving to a smaller display causes the requested size to fit that display.
Manual OS resizing is retained until a geometry preference or monitor bound changes.
The existing responsive UI-scale policy still applies.

Borderless uses the existing desktop resolution. There are no exclusive-fullscreen
video modes or graphics-quality controls. Monitor selection remains OS-managed in
this alpha: return to windowed mode, move the window, then enable borderless.
Monitor entity IDs and indices are not stable identifiers across launches; an
in-game selector is deferred until topology changes and Windows, X11 and Wayland
behavior have been validated. Wayland compositors control window placement and may
ignore centering requests. Browser builds do not expose these desktop controls.

Apply previews mode and size changes for 15 seconds. Keep confirms; Revert, Escape,
or timeout restores the last confirmed preferences. The confirmation appears above
all menus and uses wall time even when gameplay is paused. Unconfirmed values are
never saved. A saved borderless launch asks for confirmation again so a changed
desktop configuration can recover automatically to a fitted window.

Preferences live in `display-settings.ron` beside `ui-settings.ron` in the save
directory. The version-1 RON format defaults missing fields, sanitizes unsupported
sizes and frame limits, and falls back to defaults for malformed data or unknown
versions. Writes use the existing atomic file writer. Failed writes show a status
message in Display and retry every two seconds. Removing this file restores defaults.

## Packaged alpha smoke tests — pending

These require interactive packaged builds and real display hardware. Headless
tests do not establish GPU presentation behavior or compositor compatibility.
No Windows or Linux packaged manual result has been recorded for this change.

Run the following on Windows and Linux (X11 and Wayland where supported), recording
package revision, OS/compositor, GPU/driver, monitor resolution/refresh and OS scale:

- Launch without preferences; verify a visible fitted window and reachable menus.
- Apply window sizes and borderless; exercise Keep, Revert, Escape and a full timeout.
  Repeat while paused and with another settings tab open during the preview.
- Close during an unconfirmed preview, relaunch, and verify only confirmed settings
  return. Relaunch saved borderless and let the confirmation expire.
- Test VSync on/off and every frame preset, including Unlimited. Check observed FPS
  with available GPU headroom and confirm gameplay speed does not change.
- Move across monitors with different DPI/refresh rates; enter/leave borderless.
  Disconnect a monitor, relaunch, and check recovery and available window sizes.
- Check UI scales 75–200% at small and large window sizes, including scrolling and
  the confirmation buttons. Check native OS resize/maximize behavior.
- Try missing, truncated, unknown-version and out-of-range preference files.
  Try a read-only settings directory and confirm the game remains usable.

Release sign-off remains pending until both platform package tests pass.
