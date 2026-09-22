# Accessibility (alpha)

This pass expands accessibility beyond UI scale and high contrast, focusing on
core playability and information clarity. It is not a certification; known gaps
are listed at the end.

## Settings (persisted, versioned)

`ui-settings.ron` is versioned (`version: 1`). Unknown future versions load
safe runtime defaults without rewriting the file, so newer-version data
survives a downgrade; legacy files without a version keep their scale/contrast,
gain defaults for the newer options, and are upgraded to a versioned record on
load.

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
- Status symbols (default ON): prefixes machine, threat, and build status
  with distinct ASCII tags so state never depends on color alone.

All three accessibility toggles use 44px minimum hit targets and live in the
Settings Accessibility tab with Apply/Reset semantics shared with other tabs.

## Color-independent status

| Surface | Color-only risk | Alternative |
|---|---|---|
| Machine guidance | Working green vs blocked amber/red | `[>]`/`[=]`/`[F!]`/`[P!]`/... prefix + full sentence (e.g. `[P!] No power — ...`) |
| Threat alerts | Orange/red card hues | `[~]`/`[!]`/`[!!]`/`[X]`/`[?]`/`[+]` prefix + label text + threat panel counts |
| Rail signals | Green/yellow/red lamps | Lamp luminance orders reserved > clear > blocked so the ordering survives hue merge; no text tag in the world view (known gap) |
| Circuit wires | Red vs green | Green draws thicker (3.5px vs 2.0px) plus lateral offset when paired |
| Build validity | Green/red ghost tint | `[OK]`/`[X]` prefix on build status text + issue list; footprint tiles tint per-issue |
| Map overlays | Overlay hues | Numbered text toggles (`1 Pollution`...), distinct marker sizes (bases 10px vs raids 8px) |
| Selection | Selected border hue | Selected slots also draw a 3px border vs 1px unselected, plus background change |
| Enemies | Red units | Distinct square sprites + threat panel counts + map markers; audio warning always paired with cards |

Lightness ordering is checked headlessly (`status_colors_distinguishable`,
rail/circuit tests): the helper compares approximate sRGB-channel brightness,
not linearized luminance, and the 0.08 threshold is a heuristic rather than a
protanopia/deuteranopia/tritanopia simulation. Hue conventions (railway
red/yellow/green, wire red/green) are kept; wired shape and text alternatives
reduce hue reliance where they exist (machines, threats, build status, wire
thickness, selection width).

## Alerts are never audio-only

| Sound | Visual equivalent |
|---|---|
| Enemy warning | Threat alert card + threat panel |
| Rocket seal/launch | Launch banner + silo status text |
| Research complete | Technology panel unlock state |
| Craft complete | Crafting queue completion |
| Manual mine tick/complete | Progress bar / inventory gain message |
| Place / place error | Build status text + ghost preview tint |

This mapping is documentation for the presenting UI (threat cards, launch
banner, technology panel, crafting queue, mining progress, build status);
no essential alert is audio-only.

## Hit targets and focus

Sizes below are logical pixels: they scale with the user's chosen interface
scale (75–200%), matching how CSS px behave under browser zoom — the WCAG
target-size guideline is likewise scale-independent.

- Accessibility toggles: min 52x44px.
- Settings tabs/actions: min 102x44px.
- Layout uses Bevy UI buttons with visible selected/hover/pressed states;
  text inputs use `InputFocus` with explicit focus set/clear.

## High-contrast scope

High contrast remaps UI `TextColor`, `BackgroundColor`, and `BorderColor`,
including newly added or changed nodes. It does not remap world-space entity,
wire, signal, or overlay sprite hues; those rely on the symbol/shape system
above (rail signals: luminance ordering only) plus world-label enlargement.

## Known limitations (alpha)

- Rail-signal aspect has no world-space text alternative; only lamp luminance
  ordering distinguishes it when hues merge.
- No screen reader or OS high-contrast theme integration.
- No keyboard-only full playthrough; world placement still needs a pointer.
- No color-blind simulation preview in-game; automated checks cover a
  lightness-ordering heuristic and glyph uniqueness only, not formal CVD
  verification.
- World sprite hues are not remapped by high-contrast mode (see scope).
- Reduced-motion currently covers rocket-rise interpolation only; no other
  motion sources were found to gate.
- Text scaling follows UI scale; OS font-scaling hooks are not integrated.
