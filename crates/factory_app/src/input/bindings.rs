use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::save_load::SaveLoadConfig;

const BINDINGS_FILE_VERSION: u32 = 1;
const PERSISTENCE_RETRY_DELAY: Duration = Duration::from_secs(1);
const WORLD: u8 = 1;
const MAP: u8 = 2;
const ALL_CONTEXTS: u8 = WORLD | MAP;

/// A player intent. Gameplay systems ask for these actions instead of knowing
/// which physical key or mouse button currently produces them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum InputAction {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    Primary,
    MapDrag,
    Secondary,
    Alternate,
    CancelPause,
    RotateRepair,
    Hotbar1,
    Hotbar2,
    Hotbar3,
    Hotbar4,
    Hotbar5,
    Hotbar6,
    Hotbar7,
    Hotbar8,
    Hotbar9,
    Hotbar10,
    OpenBuildMenu,
    OpenBlueprintLibrary,
    OpenCrafting,
    OpenEquipment,
    OpenMap,
    OpenProduction,
    OpenTechnology,
    OpenAudioSettings,
    OpenGameplaySettings,
    CopyBlueprint,
    PasteBlueprint,
    DeconstructionPlanner,
    RedWire,
    GreenWire,
    QuickSave,
    QuickLoad,
    TrainDrive,
    TrainBrake,
    ToggleDebugReveal,
    ToggleDebugOverlay,
    ToggleRailOverlay,
    MapCenterPlayer,
    MapOverlay1,
    MapOverlay2,
    MapOverlay3,
    MapOverlay4,
    MapOverlay5,
    MapOverlay6,
}

impl InputAction {
    pub const ALL: [Self; 48] = [
        Self::MoveUp,
        Self::MoveDown,
        Self::MoveLeft,
        Self::MoveRight,
        Self::Primary,
        Self::MapDrag,
        Self::Secondary,
        Self::Alternate,
        Self::CancelPause,
        Self::RotateRepair,
        Self::Hotbar1,
        Self::Hotbar2,
        Self::Hotbar3,
        Self::Hotbar4,
        Self::Hotbar5,
        Self::Hotbar6,
        Self::Hotbar7,
        Self::Hotbar8,
        Self::Hotbar9,
        Self::Hotbar10,
        Self::OpenBuildMenu,
        Self::OpenBlueprintLibrary,
        Self::OpenCrafting,
        Self::OpenEquipment,
        Self::OpenMap,
        Self::OpenProduction,
        Self::OpenTechnology,
        Self::OpenAudioSettings,
        Self::OpenGameplaySettings,
        Self::CopyBlueprint,
        Self::PasteBlueprint,
        Self::DeconstructionPlanner,
        Self::RedWire,
        Self::GreenWire,
        Self::QuickSave,
        Self::QuickLoad,
        Self::TrainDrive,
        Self::TrainBrake,
        Self::ToggleDebugReveal,
        Self::ToggleDebugOverlay,
        Self::ToggleRailOverlay,
        Self::MapCenterPlayer,
        Self::MapOverlay1,
        Self::MapOverlay2,
        Self::MapOverlay3,
        Self::MapOverlay4,
        Self::MapOverlay5,
        Self::MapOverlay6,
    ];

    pub const HOTBAR: [Self; 10] = [
        Self::Hotbar1,
        Self::Hotbar2,
        Self::Hotbar3,
        Self::Hotbar4,
        Self::Hotbar5,
        Self::Hotbar6,
        Self::Hotbar7,
        Self::Hotbar8,
        Self::Hotbar9,
        Self::Hotbar10,
    ];

    pub const MAP_OVERLAYS: [Self; 6] = [
        Self::MapOverlay1,
        Self::MapOverlay2,
        Self::MapOverlay3,
        Self::MapOverlay4,
        Self::MapOverlay5,
        Self::MapOverlay6,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::MoveUp => "Move up",
            Self::MoveDown => "Move down",
            Self::MoveLeft => "Move left",
            Self::MoveRight => "Move right",
            Self::Primary => "Primary action",
            Self::MapDrag => "Map: drag",
            Self::Secondary => "Mine / secondary action",
            Self::Alternate => "Alternate mode",
            Self::CancelPause => "Cancel / pause",
            Self::RotateRepair => "Rotate / repair",
            Self::Hotbar1 => "Hotbar slot 1",
            Self::Hotbar2 => "Hotbar slot 2",
            Self::Hotbar3 => "Hotbar slot 3",
            Self::Hotbar4 => "Hotbar slot 4",
            Self::Hotbar5 => "Hotbar slot 5",
            Self::Hotbar6 => "Hotbar slot 6",
            Self::Hotbar7 => "Hotbar slot 7",
            Self::Hotbar8 => "Hotbar slot 8",
            Self::Hotbar9 => "Hotbar slot 9",
            Self::Hotbar10 => "Hotbar slot 10",
            Self::OpenBuildMenu => "Build menu",
            Self::OpenBlueprintLibrary => "Blueprint library",
            Self::OpenCrafting => "Crafting",
            Self::OpenEquipment => "Equipment",
            Self::OpenMap => "Map",
            Self::OpenProduction => "Production statistics",
            Self::OpenTechnology => "Technology",
            Self::OpenAudioSettings => "Audio settings",
            Self::OpenGameplaySettings => "Gameplay settings",
            Self::CopyBlueprint => "Copy blueprint",
            Self::PasteBlueprint => "Paste blueprint",
            Self::DeconstructionPlanner => "Deconstruction planner",
            Self::RedWire => "Red wire tool",
            Self::GreenWire => "Green wire tool",
            Self::QuickSave => "Quicksave",
            Self::QuickLoad => "Quickload",
            Self::TrainDrive => "Drive / reverse train",
            Self::TrainBrake => "Brake train",
            Self::ToggleDebugReveal => "Debug map reveal",
            Self::ToggleDebugOverlay => "Performance overlay",
            Self::ToggleRailOverlay => "Rail connectivity overlay",
            Self::MapCenterPlayer => "Map: center player",
            Self::MapOverlay1 => "Map: pollution",
            Self::MapOverlay2 => "Map: resources",
            Self::MapOverlay3 => "Map: power networks",
            Self::MapOverlay4 => "Map: production problems",
            Self::MapOverlay5 => "Map: enemies",
            Self::MapOverlay6 => "Map: construction plans",
        }
    }

    pub const fn category(self) -> &'static str {
        match self {
            Self::MoveUp | Self::MoveDown | Self::MoveLeft | Self::MoveRight => "MOVEMENT",
            Self::Hotbar1
            | Self::Hotbar2
            | Self::Hotbar3
            | Self::Hotbar4
            | Self::Hotbar5
            | Self::Hotbar6
            | Self::Hotbar7
            | Self::Hotbar8
            | Self::Hotbar9
            | Self::Hotbar10
            | Self::OpenBuildMenu
            | Self::OpenBlueprintLibrary
            | Self::CopyBlueprint
            | Self::PasteBlueprint
            | Self::DeconstructionPlanner
            | Self::RedWire
            | Self::GreenWire
            | Self::RotateRepair => "BUILDING & TOOLS",
            Self::OpenMap
            | Self::OpenProduction
            | Self::OpenTechnology
            | Self::OpenCrafting
            | Self::OpenEquipment
            | Self::OpenAudioSettings
            | Self::OpenGameplaySettings
            | Self::CancelPause => "PANELS",
            Self::MapCenterPlayer
            | Self::MapDrag
            | Self::MapOverlay1
            | Self::MapOverlay2
            | Self::MapOverlay3
            | Self::MapOverlay4
            | Self::MapOverlay5
            | Self::MapOverlay6 => "MAP",
            Self::QuickSave | Self::QuickLoad => "SAVE & LOAD",
            Self::TrainDrive | Self::TrainBrake => "TRAINS",
            Self::ToggleDebugReveal | Self::ToggleDebugOverlay | Self::ToggleRailOverlay => "DEBUG",
            Self::Primary | Self::Secondary | Self::Alternate => "WORLD",
        }
    }

    const fn contexts(self) -> u8 {
        match self {
            Self::MapCenterPlayer
            | Self::MapDrag
            | Self::MapOverlay1
            | Self::MapOverlay2
            | Self::MapOverlay3
            | Self::MapOverlay4
            | Self::MapOverlay5
            | Self::MapOverlay6 => MAP,
            Self::CancelPause => ALL_CONTEXTS,
            _ => WORLD,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Modifiers {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BindingInput {
    Key(KeyCode),
    Mouse(MouseButton),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputBinding {
    pub input: BindingInput,
    #[serde(default)]
    pub modifiers: Modifiers,
}

impl InputBinding {
    pub const fn key(key: KeyCode) -> Self {
        Self {
            input: BindingInput::Key(key),
            modifiers: Modifiers {
                control: false,
                alt: false,
                shift: false,
                super_key: false,
            },
        }
    }

    pub const fn control(key: KeyCode) -> Self {
        Self {
            input: BindingInput::Key(key),
            modifiers: Modifiers {
                control: true,
                alt: false,
                shift: false,
                super_key: false,
            },
        }
    }

    pub const fn mouse(button: MouseButton) -> Self {
        Self {
            input: BindingInput::Mouse(button),
            modifiers: Modifiers {
                control: false,
                alt: false,
                shift: false,
                super_key: false,
            },
        }
    }

    pub fn display_name(self) -> String {
        let mut parts = Vec::new();
        if self.modifiers.control {
            parts.push("Ctrl".to_string());
        }
        if self.modifiers.alt {
            parts.push("Alt".to_string());
        }
        if self.modifiers.shift {
            parts.push("Shift".to_string());
        }
        if self.modifiers.super_key {
            parts.push("Super".to_string());
        }
        parts.push(match self.input {
            BindingInput::Key(key) => key_name(key),
            BindingInput::Mouse(button) => mouse_name(button),
        });
        parts.join("+")
    }
}

/// The active binding registry. An action can have multiple physical inputs;
/// rebinding from the controls screen replaces that action with one new chord.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct ActionBindings {
    bindings: BTreeMap<InputAction, Vec<InputBinding>>,
    settings_path: PathBuf,
}

impl Default for ActionBindings {
    fn default() -> Self {
        Self {
            bindings: default_binding_map(),
            settings_path: PathBuf::new(),
        }
    }
}

impl ActionBindings {
    pub fn bindings(&self, action: InputAction) -> &[InputBinding] {
        self.bindings.get(&action).map_or(&[], Vec::as_slice)
    }

    pub fn display_name(&self, action: InputAction) -> String {
        self.bindings(action)
            .iter()
            .map(|binding| binding.display_name())
            .collect::<Vec<_>>()
            .join(" / ")
    }

    pub fn rebind(
        &mut self,
        action: InputAction,
        binding: InputBinding,
    ) -> Result<(), BindingConflict> {
        if let Some(conflicting_action) = InputAction::ALL.into_iter().find(|other| {
            *other != action
                && action.contexts() & other.contexts() != 0
                && self.bindings(*other).contains(&binding)
        }) {
            return Err(BindingConflict {
                action: conflicting_action,
            });
        }
        self.bindings.insert(action, vec![binding]);
        Ok(())
    }

    pub fn reset_to_defaults(&mut self) {
        self.bindings = default_binding_map();
    }

    fn pressed(
        &self,
        action: InputAction,
        keyboard: Option<&ButtonInput<KeyCode>>,
        mouse: Option<&ButtonInput<MouseButton>>,
    ) -> bool {
        self.bindings(action)
            .iter()
            .any(|binding| binding.matches(keyboard, mouse, ButtonPhase::Pressed))
    }

    fn just_pressed(
        &self,
        action: InputAction,
        keyboard: Option<&ButtonInput<KeyCode>>,
        mouse: Option<&ButtonInput<MouseButton>>,
    ) -> bool {
        self.bindings(action)
            .iter()
            .any(|binding| binding.matches(keyboard, mouse, ButtonPhase::JustPressed))
    }

    fn just_released(
        &self,
        action: InputAction,
        keyboard: Option<&ButtonInput<KeyCode>>,
        mouse: Option<&ButtonInput<MouseButton>>,
    ) -> bool {
        self.bindings(action)
            .iter()
            .any(|binding| binding.matches(keyboard, mouse, ButtonPhase::JustReleased))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BindingConflict {
    pub action: InputAction,
}

#[derive(SystemParam)]
pub struct ActionInput<'w> {
    bindings: Res<'w, ActionBindings>,
    keyboard: Option<Res<'w, ButtonInput<KeyCode>>>,
    mouse: Option<Res<'w, ButtonInput<MouseButton>>>,
}

impl ActionInput<'_> {
    pub fn pressed(&self, action: InputAction) -> bool {
        self.bindings
            .pressed(action, self.keyboard.as_deref(), self.mouse.as_deref())
    }

    pub fn just_pressed(&self, action: InputAction) -> bool {
        self.bindings
            .just_pressed(action, self.keyboard.as_deref(), self.mouse.as_deref())
    }

    pub fn just_released(&self, action: InputAction) -> bool {
        self.bindings
            .just_released(action, self.keyboard.as_deref(), self.mouse.as_deref())
    }

    /// Escape retains its conventional text-entry dismissal behavior even if
    /// the gameplay cancel action is remapped. No other action is exposed
    /// while editable text owns focus.
    pub fn text_entry_cancelled(&self) -> bool {
        self.keyboard
            .as_deref()
            .is_some_and(|keyboard| keyboard.just_pressed(KeyCode::Escape))
    }
}

#[derive(Clone, Copy)]
enum ButtonPhase {
    Pressed,
    JustPressed,
    JustReleased,
}

impl InputBinding {
    fn matches(
        self,
        keyboard: Option<&ButtonInput<KeyCode>>,
        mouse: Option<&ButtonInput<MouseButton>>,
        phase: ButtonPhase,
    ) -> bool {
        if !modifiers_match(self.modifiers, keyboard, self.input) {
            return false;
        }
        match self.input {
            BindingInput::Key(key) => keyboard.is_some_and(|input| match phase {
                ButtonPhase::Pressed => input.pressed(key),
                ButtonPhase::JustPressed => input.just_pressed(key),
                ButtonPhase::JustReleased => input.just_released(key),
            }),
            BindingInput::Mouse(button) => mouse.is_some_and(|input| match phase {
                ButtonPhase::Pressed => input.pressed(button),
                ButtonPhase::JustPressed => input.just_pressed(button),
                ButtonPhase::JustReleased => input.just_released(button),
            }),
        }
    }
}

fn modifiers_match(
    required: Modifiers,
    keyboard: Option<&ButtonInput<KeyCode>>,
    input: BindingInput,
) -> bool {
    let mut held = current_modifiers(keyboard);
    if let BindingInput::Key(key) = input {
        match key {
            KeyCode::ControlLeft | KeyCode::ControlRight => held.control = false,
            KeyCode::AltLeft | KeyCode::AltRight => held.alt = false,
            KeyCode::ShiftLeft | KeyCode::ShiftRight => held.shift = false,
            KeyCode::SuperLeft | KeyCode::SuperRight => held.super_key = false,
            _ => {}
        }
    }
    required.control == held.control
        && required.alt == held.alt
        && required.super_key == held.super_key
        && (!required.shift || held.shift)
}

pub fn current_modifiers(keyboard: Option<&ButtonInput<KeyCode>>) -> Modifiers {
    let Some(keyboard) = keyboard else {
        return Modifiers::default();
    };
    Modifiers {
        control: keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight),
        alt: keyboard.pressed(KeyCode::AltLeft) || keyboard.pressed(KeyCode::AltRight),
        shift: keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight),
        super_key: keyboard.pressed(KeyCode::SuperLeft) || keyboard.pressed(KeyCode::SuperRight),
    }
}

fn default_binding_map() -> BTreeMap<InputAction, Vec<InputBinding>> {
    use InputAction as A;
    use KeyCode as K;
    use MouseButton as M;

    let entries: [(A, Vec<InputBinding>); 48] = [
        (
            A::MoveUp,
            vec![InputBinding::key(K::KeyW), InputBinding::key(K::ArrowUp)],
        ),
        (
            A::MoveDown,
            vec![InputBinding::key(K::KeyS), InputBinding::key(K::ArrowDown)],
        ),
        (
            A::MoveLeft,
            vec![InputBinding::key(K::KeyA), InputBinding::key(K::ArrowLeft)],
        ),
        (
            A::MoveRight,
            vec![InputBinding::key(K::KeyD), InputBinding::key(K::ArrowRight)],
        ),
        (A::Primary, vec![InputBinding::mouse(M::Left)]),
        (
            A::MapDrag,
            vec![InputBinding::mouse(M::Left), InputBinding::mouse(M::Middle)],
        ),
        (A::Secondary, vec![InputBinding::mouse(M::Right)]),
        (
            A::Alternate,
            vec![
                InputBinding::key(K::ShiftLeft),
                InputBinding::key(K::ShiftRight),
            ],
        ),
        (A::CancelPause, vec![InputBinding::key(K::Escape)]),
        (A::RotateRepair, vec![InputBinding::key(K::KeyR)]),
        (A::Hotbar1, vec![InputBinding::key(K::Digit1)]),
        (A::Hotbar2, vec![InputBinding::key(K::Digit2)]),
        (A::Hotbar3, vec![InputBinding::key(K::Digit3)]),
        (A::Hotbar4, vec![InputBinding::key(K::Digit4)]),
        (A::Hotbar5, vec![InputBinding::key(K::Digit5)]),
        (A::Hotbar6, vec![InputBinding::key(K::Digit6)]),
        (A::Hotbar7, vec![InputBinding::key(K::Digit7)]),
        (A::Hotbar8, vec![InputBinding::key(K::Digit8)]),
        (A::Hotbar9, vec![InputBinding::key(K::Digit9)]),
        (A::Hotbar10, vec![InputBinding::key(K::Digit0)]),
        (A::OpenBuildMenu, vec![InputBinding::key(K::KeyB)]),
        (
            A::OpenBlueprintLibrary,
            vec![InputBinding::control(K::KeyB)],
        ),
        (A::OpenCrafting, vec![InputBinding::key(K::KeyC)]),
        (A::OpenEquipment, vec![InputBinding::key(K::KeyE)]),
        (A::OpenMap, vec![InputBinding::key(K::KeyM)]),
        (A::OpenProduction, vec![InputBinding::key(K::KeyP)]),
        (A::OpenTechnology, vec![InputBinding::key(K::KeyT)]),
        (A::OpenAudioSettings, vec![InputBinding::key(K::KeyO)]),
        (A::OpenGameplaySettings, vec![InputBinding::key(K::KeyN)]),
        (A::CopyBlueprint, vec![InputBinding::control(K::KeyC)]),
        (A::PasteBlueprint, vec![InputBinding::control(K::KeyV)]),
        (A::DeconstructionPlanner, vec![InputBinding::key(K::KeyX)]),
        // C already opens crafting in this context. G gives the red wire tool
        // a reachable, conflict-free default while green remains on V.
        (A::RedWire, vec![InputBinding::key(K::KeyG)]),
        (A::GreenWire, vec![InputBinding::key(K::KeyV)]),
        (A::QuickSave, vec![InputBinding::key(K::F5)]),
        (A::QuickLoad, vec![InputBinding::key(K::F9)]),
        (A::TrainDrive, vec![InputBinding::key(K::F8)]),
        (A::TrainBrake, vec![InputBinding::key(K::F10)]),
        (A::ToggleDebugReveal, vec![InputBinding::key(K::F3)]),
        (A::ToggleDebugOverlay, vec![InputBinding::key(K::F4)]),
        (A::ToggleRailOverlay, vec![InputBinding::key(K::F7)]),
        (A::MapCenterPlayer, vec![InputBinding::key(K::KeyF)]),
        (A::MapOverlay1, vec![InputBinding::key(K::Digit1)]),
        (A::MapOverlay2, vec![InputBinding::key(K::Digit2)]),
        (A::MapOverlay3, vec![InputBinding::key(K::Digit3)]),
        (A::MapOverlay4, vec![InputBinding::key(K::Digit4)]),
        (A::MapOverlay5, vec![InputBinding::key(K::Digit5)]),
        (A::MapOverlay6, vec![InputBinding::key(K::Digit6)]),
    ];
    entries.into_iter().collect()
}

#[derive(Resource, Default)]
pub struct BindingPersistenceState {
    last_saved: Option<BindingsFile>,
    retry_after: Option<Duration>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingsFile {
    pub version: u32,
    pub bindings: Vec<ActionBindingFile>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionBindingFile {
    pub action: InputAction,
    pub bindings: Vec<InputBinding>,
}

impl BindingsFile {
    fn from_registry(registry: &ActionBindings) -> Self {
        Self {
            version: BINDINGS_FILE_VERSION,
            bindings: InputAction::ALL
                .into_iter()
                .map(|action| ActionBindingFile {
                    action,
                    bindings: registry.bindings(action).to_vec(),
                })
                .collect(),
        }
    }

    fn into_registry(self, path: PathBuf) -> Option<ActionBindings> {
        if self.version != BINDINGS_FILE_VERSION {
            return None;
        }
        let mut registry = ActionBindings {
            settings_path: path,
            ..Default::default()
        };
        for entry in self.bindings {
            if entry.bindings.is_empty() {
                continue;
            }
            registry.bindings.insert(entry.action, entry.bindings);
        }
        // Edited files receive the same conflict validation as the UI. Invalid
        // entries fall back to the corresponding default rather than making
        // multiple actions fire together.
        let defaults = ActionBindings::default();
        for action in InputAction::ALL {
            let bindings = registry.bindings(action).to_vec();
            let has_conflict = bindings.iter().any(|binding| {
                InputAction::ALL.into_iter().any(|other| {
                    other != action
                        && action.contexts() & other.contexts() != 0
                        && registry.bindings(other).contains(binding)
                })
            });
            if has_conflict {
                registry
                    .bindings
                    .insert(action, defaults.bindings(action).to_vec());
            }
        }
        Some(registry)
    }
}

pub(crate) fn load_persisted_bindings(
    config: Res<SaveLoadConfig>,
    mut registry: ResMut<ActionBindings>,
    mut persistence: ResMut<BindingPersistenceState>,
) {
    let path = bindings_path(&config);
    let loaded = read_bindings_file(&path)
        .and_then(|file| file.into_registry(path.clone()))
        .unwrap_or_else(|| ActionBindings {
            settings_path: path,
            ..Default::default()
        });
    *registry = loaded;
    persistence.last_saved = Some(BindingsFile::from_registry(&registry));
}

pub(crate) fn save_bindings_if_changed(
    time: Res<Time<Real>>,
    registry: Res<ActionBindings>,
    mut persistence: ResMut<BindingPersistenceState>,
) {
    if registry.settings_path.as_os_str().is_empty() {
        return;
    }
    let file = BindingsFile::from_registry(&registry);
    if persistence.last_saved.as_ref() == Some(&file) {
        persistence.retry_after = None;
        return;
    }
    let now = time.elapsed();
    if !registry.is_changed()
        && persistence
            .retry_after
            .is_none_or(|retry_after| now < retry_after)
    {
        return;
    }
    if write_bindings_file(&registry.settings_path, &file).is_ok() {
        persistence.last_saved = Some(file);
        persistence.retry_after = None;
    } else {
        persistence.retry_after = Some(now + PERSISTENCE_RETRY_DELAY);
    }
}

pub fn read_bindings_file(path: &Path) -> Option<BindingsFile> {
    ron::from_str(&fs::read_to_string(path).ok()?).ok()
}

pub fn write_bindings_file(path: &Path, file: &BindingsFile) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = ron::ser::to_string_pretty(file, ron::ser::PrettyConfig::default())
        .map_err(std::io::Error::other)?;
    fs::write(path, text)
}

pub fn bindings_path(config: &SaveLoadConfig) -> PathBuf {
    config.root_dir.join("controls.ron")
}

fn key_name(key: KeyCode) -> String {
    let raw = format!("{key:?}");
    if let Some(letter) = raw.strip_prefix("Key") {
        return letter.to_string();
    }
    if let Some(digit) = raw.strip_prefix("Digit") {
        return digit.to_string();
    }
    match key {
        KeyCode::ArrowUp => "Up".into(),
        KeyCode::ArrowDown => "Down".into(),
        KeyCode::ArrowLeft => "Left".into(),
        KeyCode::ArrowRight => "Right".into(),
        KeyCode::ShiftLeft => "Left Shift".into(),
        KeyCode::ShiftRight => "Right Shift".into(),
        KeyCode::ControlLeft => "Left Ctrl".into(),
        KeyCode::ControlRight => "Right Ctrl".into(),
        KeyCode::AltLeft => "Left Alt".into(),
        KeyCode::AltRight => "Right Alt".into(),
        _ => raw,
    }
}

fn mouse_name(button: MouseButton) -> String {
    match button {
        MouseButton::Left => "Left Mouse".into(),
        MouseButton::Right => "Right Mouse".into(),
        MouseButton::Middle => "Middle Mouse".into(),
        MouseButton::Back => "Mouse Back".into(),
        MouseButton::Forward => "Mouse Forward".into(),
        MouseButton::Other(number) => format!("Mouse {number}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_cover_every_action_without_same_context_conflicts() {
        let registry = ActionBindings::default();
        for action in InputAction::ALL {
            assert!(!registry.bindings(action).is_empty(), "missing {action:?}");
            for binding in registry.bindings(action) {
                assert!(
                    !InputAction::ALL.into_iter().any(|other| {
                        other != action
                            && action.contexts() & other.contexts() != 0
                            && registry.bindings(other).contains(binding)
                    }),
                    "{action:?} conflicts on {binding:?}"
                );
            }
        }
    }

    #[test]
    fn remapping_replaces_defaults_and_reset_restores_them() {
        let mut registry = ActionBindings::default();
        let defaults = registry.bindings(InputAction::MoveUp).to_vec();
        let replacement = InputBinding::key(KeyCode::KeyI);
        registry
            .rebind(InputAction::MoveUp, replacement)
            .expect("unused key should bind");
        assert_eq!(registry.bindings(InputAction::MoveUp), &[replacement]);
        registry.reset_to_defaults();
        assert_eq!(registry.bindings(InputAction::MoveUp), defaults);
    }

    #[test]
    fn conflicts_are_context_sensitive() {
        let mut registry = ActionBindings::default();
        let world_binding = registry.bindings(InputAction::OpenBuildMenu)[0];
        assert_eq!(
            registry.rebind(InputAction::MoveUp, world_binding),
            Err(BindingConflict {
                action: InputAction::OpenBuildMenu
            })
        );
        assert!(
            registry
                .rebind(InputAction::MapCenterPlayer, world_binding)
                .is_ok(),
            "the same key is valid in the exclusive full-map context"
        );
    }

    #[test]
    fn action_queries_follow_the_remapped_key() {
        let mut registry = ActionBindings::default();
        registry
            .rebind(InputAction::MoveUp, InputBinding::key(KeyCode::KeyI))
            .unwrap();
        let mut keyboard = ButtonInput::default();
        keyboard.press(KeyCode::KeyW);
        assert!(!registry.pressed(InputAction::MoveUp, Some(&keyboard), None));
        keyboard.press(KeyCode::KeyI);
        assert!(registry.pressed(InputAction::MoveUp, Some(&keyboard), None));
    }

    #[test]
    fn modifier_keys_can_be_bindings_and_modifiers_can_form_chords() {
        let mut keyboard = ButtonInput::default();
        keyboard.press(KeyCode::ControlLeft);
        assert!(InputBinding::key(KeyCode::ControlLeft).matches(
            Some(&keyboard),
            None,
            ButtonPhase::Pressed
        ));

        keyboard.press(KeyCode::KeyI);
        assert!(InputBinding::control(KeyCode::KeyI).matches(
            Some(&keyboard),
            None,
            ButtonPhase::Pressed
        ));
        assert!(!InputBinding::key(KeyCode::KeyI).matches(
            Some(&keyboard),
            None,
            ButtonPhase::Pressed
        ));
    }

    #[test]
    fn versioned_file_round_trips_and_rejects_unknown_versions() {
        let root = std::env::temp_dir().join(format!(
            "factory-controls-test-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("unnamed")
        ));
        let path = root.join("controls.ron");
        let mut registry = ActionBindings::default();
        registry
            .rebind(InputAction::MoveUp, InputBinding::key(KeyCode::KeyI))
            .unwrap();
        let file = BindingsFile::from_registry(&registry);
        write_bindings_file(&path, &file).unwrap();
        let loaded = read_bindings_file(&path)
            .unwrap()
            .into_registry(path.clone())
            .unwrap();
        assert_eq!(
            loaded.bindings(InputAction::MoveUp),
            &[InputBinding::key(KeyCode::KeyI)]
        );

        let mut future = file;
        future.version += 1;
        assert!(future.into_registry(path).is_none());
        let _ = fs::remove_dir_all(root);
    }
}
