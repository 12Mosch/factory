# Accessibility (alpha)

This pass expands accessibility beyond UI scale and high contrast, focusing on
core playability and information clarity. It is not a certification; known gaps
are listed at the end.

## Settings (persisted, versioned)

`ui-settings.ron` is versioned (`version: 1`). Unknown versions reset to safe
defaults; legacy files without a version keep their scale/contrast and gain
defaults for the newer options.

- Interface scale: 75–200%, responsive-clamped so the logical viewport stays
  usable. Persisted.
- Readable high contrast: remaps UI text/background/border colors and enlarges
  world labels with a shadow. Applies to existing UI (`refresh_high_contrast_palette`)
  and to newly spawned or recolored UI (`update_high_contrast_palette`).
  World-space sprite hues are intentionally not remapped; status there uses
  symbols instead (below).
- Reduce motion and flashing: disables nonessential presentation smoothing only
  (currently rocket-rise frame interpolation via `reduced_motion_overstep`).
  The fixed-step simulation is unchanged. No repeating flashes, screen pulses,
  or camera shake exist in alpha; the toggle future-proofs upcoming effects.
- Status symbols (default ON): prefixes machine, threat, signal, and build
  status with distinct ASCII tags so state never depends on color alone.

All three accessibility toggles use 44px minimum hit targets and live in the
Settings Accessibility tab with Apply/Reset semantics shared with other tabs.

## Color-independent status

| Surface | Color-only risk | Alternative |
|---|---|---|
| Machine guidance | Working green vs blocked amber/red | `[>]`/`[=]`/`[F!]`/`[P!]`/... prefix + full sentence (e.g. `[P!] No power — ...`) |
| Threat alerts | Orange/red card hues | `[~]`/`[!]`/`[!!]`/`[X]`/`[?]`/`[+]` prefix + label text + threat panel counts |
| Rail signals | Green/yellow/red lamps | `[GO]`/`[WAIT]`/`[STOP]` glyphs + Clear/Caution/Stop labels; lamp luminance orders reserved > clear > blocked; lamp size varies by aspect |
| Circuit wires | Red vs green | Green draws thicker (3.5px vs 2.0px) plus lateral offset when paired; `[R]`/`[G]` glyphs for legends |
| Build validity | Green/red ghost tint | `[OK]`/`[X]` prefix on build status text + issue list; footprint tiles tint per-issue |
| Map overlays | Overlay hues | Numbered text toggles (`1 Pollution`...), distinct marker sizes (bases 10px vs raids 8px), `[P]`/`[R]`/`[E]`/`[!]`/`[X]`/`[C]` legend tags |
| Selection | Selected border hue | Selected slots also draw a 3px border vs 1px unselected, plus background change |
| Enemies | Red units | Distinct square sprites + threat panel counts + map markers; audio warning always paired with cards |

Luminance separation is verified headlessly (`status_colors_distinguishable`,
rail/circuit tests). Hue conventions (railway red/yellow/green, wire red/green)
are kept; shape and text carry the distinction under deuteranopia/protanopia/
tritanopia.

## Alerts are never audio-only

| Sound | Visual equivalent |
|---|---|
| Enemy warning | Threat alert card + threat panel |
| Rocket seal/launch | Launch banner + silo status text |
| Research complete | Technology panel unlock state |
| Craft complete | Crafting queue completion |
| Manual mine tick/complete | Progress bar / inventory gain message |
| Place / place error | Build status text + ghost preview tint |

`sound_is_essential_alert` + `sound_visual_equivalent` encode this mapping with
a test (`essential_alerts_are_never_audio_only`).

## Hit targets and focus

- Accessibility toggles: min 52x44px.
- Settings tabs/actions: min 102x44px.
- Layout uses Bevy UI buttons with visible selected/hover/pressed states;
  text inputs use `InputFocus` with explicit focus set/clear.

## High-contrast scope

High contrast remaps UI `TextColor`, `BackgroundColor`, and `BorderColor`,
including newly added or changed nodes. It does not remap world-space entity,
wire, signal, or overlay sprite hues; those rely on the symbol/shape system
above plus world-label enlargement.

## Known limitations (alpha)

- No screen reader or OS high-contrast theme integration.
- No keyboard-only full playthrough; world placement still needs a pointer.
- No color-blind simulation preview in-game; verification is via luminance and
  glyph uniqueness tests.
- World sprite hues are not remapped by high-contrast mode (see scope).
- Reduced-motion currently covers rocket-rise interpolation only; no other
  motion sources were found to gate.
- Text scaling follows UI scale; OS font-scaling hooks are not integrated.
