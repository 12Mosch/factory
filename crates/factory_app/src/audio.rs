use bevy::audio::{AudioSink, AudioSinkPlayback, Volume};
use bevy::prelude::*;
use factory_data::EntityKind;
use factory_sim::{CraftingJob, EntityId, MachineStatus, ManualMiningProgress};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::rendering::transforms::entity_translation;
use crate::resources::{SimResource, VisibleEntityIds};
use crate::save_load::SaveLoadConfig;

const DEFAULT_VOLUME: f32 = 0.65;
const VOLUME_STEP: f32 = 0.10;
const MAX_MACHINE_LOOPS: usize = 32;
const MACHINE_LOOP_GAIN: f32 = 0.18;

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SoundEvent {
    UiClick,
    Place,
    PlaceError,
    ManualMineTick,
    ManualMineComplete,
    CraftComplete,
    ResearchComplete,
}

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct AudioSettings {
    pub muted: bool,
    pub volume: f32,
    pub settings_path: PathBuf,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            muted: false,
            volume: DEFAULT_VOLUME,
            settings_path: PathBuf::new(),
        }
    }
}

impl AudioSettings {
    pub fn effective_volume(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.volume.clamp(0.0, 1.0)
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }

    pub fn adjust_volume_steps(&mut self, steps: i32) {
        self.set_volume(self.volume + VOLUME_STEP * steps as f32);
    }

    pub fn toggle_muted(&mut self) {
        self.muted = !self.muted;
    }
}

#[derive(Resource, Default)]
pub struct AudioSettingsWindowState {
    pub open: bool,
}

#[derive(Resource, Default)]
pub struct AudioAssets {
    pub ui_click: Option<Handle<AudioSource>>,
    pub place: Option<Handle<AudioSource>>,
    pub place_error: Option<Handle<AudioSource>>,
    pub manual_mine_tick: Option<Handle<AudioSource>>,
    pub manual_mine_complete: Option<Handle<AudioSource>>,
    pub craft_complete: Option<Handle<AudioSource>>,
    pub machine_burner_loop: Option<Handle<AudioSource>>,
    pub machine_electric_loop: Option<Handle<AudioSource>>,
    pub research_complete: Option<Handle<AudioSource>>,
}

#[derive(Resource, Default)]
pub struct MachineAudioLoops {
    pub by_entity: HashMap<EntityId, Entity>,
}

#[derive(Resource, Default)]
pub struct AudioEventDedupe {
    last_played_tick: HashMap<SoundEvent, u64>,
}

#[derive(Resource, Default)]
pub struct ManualMiningAudioObserver {
    previous: Option<ManualMiningProgress>,
    active_ticks: u32,
}

#[derive(Resource, Default)]
pub struct CraftingAudioObserver {
    previous_front: Option<CraftingJob>,
    previous_len: usize,
}

#[derive(Resource, Default)]
pub struct ResearchAudioObserver {
    initialized: bool,
    unlocked: HashSet<factory_data::TechnologyId>,
}

#[derive(Resource, Default)]
pub struct AudioSettingsPersistenceState {
    last_saved: Option<AudioSettingsFile>,
}

#[derive(Component)]
pub struct MachineLoopAudio {
    pub entity_id: EntityId,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AudioSettingsFile {
    pub muted: bool,
    pub volume: f32,
}

impl AudioSettingsFile {
    fn from_settings(settings: &AudioSettings) -> Self {
        Self {
            muted: settings.muted,
            volume: settings.volume.clamp(0.0, 1.0),
        }
    }
}

pub(crate) fn load_audio_assets(
    asset_server: Option<Res<AssetServer>>,
    mut assets: ResMut<AudioAssets>,
) {
    let Some(asset_server) = asset_server else {
        return;
    };

    assets.ui_click = Some(asset_server.load("audio/ui_click.wav"));
    assets.place = Some(asset_server.load("audio/place.wav"));
    assets.place_error = Some(asset_server.load("audio/place_error.wav"));
    assets.manual_mine_tick = Some(asset_server.load("audio/manual_mine_tick.wav"));
    assets.manual_mine_complete = Some(asset_server.load("audio/manual_mine_complete.wav"));
    assets.craft_complete = Some(asset_server.load("audio/craft_complete.wav"));
    assets.machine_burner_loop = Some(asset_server.load("audio/machine_burner_loop.wav"));
    assets.machine_electric_loop = Some(asset_server.load("audio/machine_electric_loop.wav"));
    assets.research_complete = Some(asset_server.load("audio/research_complete.wav"));
}

pub(crate) fn load_persisted_audio_settings(
    config: Res<SaveLoadConfig>,
    mut settings: ResMut<AudioSettings>,
    mut persistence: ResMut<AudioSettingsPersistenceState>,
) {
    let path = settings_path(&config);
    let file = read_audio_settings_file(&path).unwrap_or_default();

    settings.settings_path = path;
    settings.muted = file.muted;
    settings.set_volume(file.volume);
    persistence.last_saved = Some(AudioSettingsFile::from_settings(&settings));
}

pub(crate) fn save_audio_settings_if_changed(
    settings: Res<AudioSettings>,
    mut persistence: ResMut<AudioSettingsPersistenceState>,
) {
    if !settings.is_changed() || settings.settings_path.as_os_str().is_empty() {
        return;
    }

    let file = AudioSettingsFile::from_settings(&settings);
    if persistence.last_saved.as_ref() == Some(&file) {
        return;
    }

    if write_audio_settings_file(&settings.settings_path, &file).is_ok() {
        persistence.last_saved = Some(file);
    }
}

pub fn read_audio_settings_file(path: &Path) -> Option<AudioSettingsFile> {
    let text = fs::read_to_string(path).ok()?;
    let mut file = ron::from_str::<AudioSettingsFile>(&text).ok()?;
    file.volume = file.volume.clamp(0.0, 1.0);
    Some(file)
}

pub fn write_audio_settings_file(
    path: &Path,
    file: &AudioSettingsFile,
) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = ron::ser::to_string_pretty(file, ron::ser::PrettyConfig::default())
        .unwrap_or_else(|_| "(muted:false,volume:0.65)".to_string());
    fs::write(path, text)
}

pub fn settings_path(config: &SaveLoadConfig) -> PathBuf {
    config.root_dir.join("settings.ron")
}

pub(crate) fn play_sound_events(
    mut commands: Commands,
    mut events: MessageReader<SoundEvent>,
    assets: Res<AudioAssets>,
    settings: Res<AudioSettings>,
    sim: Res<SimResource>,
    mut dedupe: ResMut<AudioEventDedupe>,
) {
    let effective_volume = settings.effective_volume();
    if effective_volume <= 0.0 {
        events.clear();
        return;
    }

    let tick = sim.sim.tick_count();
    for event in events.read() {
        if dedupe
            .last_played_tick
            .get(event)
            .is_some_and(|last_tick| tick.saturating_sub(*last_tick) < sound_cooldown_ticks(*event))
        {
            continue;
        }
        let Some(handle) = sound_handle(&assets, *event).cloned() else {
            continue;
        };
        dedupe.last_played_tick.insert(*event, tick);
        commands.spawn((
            AudioPlayer::new(handle),
            PlaybackSettings::DESPAWN
                .with_volume(Volume::Linear(effective_volume * one_shot_gain(*event))),
        ));
    }
}

pub(crate) fn apply_audio_settings_to_sinks(
    settings: Res<AudioSettings>,
    mut sinks: Query<(&mut AudioSink, Option<&MachineLoopAudio>)>,
) {
    if !settings.is_changed() {
        return;
    }

    let effective_volume = settings.effective_volume();
    for (mut sink, loop_marker) in &mut sinks {
        let gain = if loop_marker.is_some() {
            MACHINE_LOOP_GAIN
        } else {
            1.0
        };
        sink.set_volume(Volume::Linear(effective_volume * gain));
    }
}

pub(crate) fn observe_manual_mining_audio(
    sim: Res<SimResource>,
    mut observer: ResMut<ManualMiningAudioObserver>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let current = sim.sim.manual_mining_progress();
    let previous = observer.previous;

    if let (Some(previous), Some(current)) = (previous, current) {
        if previous.target == current.target {
            observer.active_ticks = observer.active_ticks.saturating_add(1);
            if current.progress_ticks < previous.progress_ticks {
                sounds.write(SoundEvent::ManualMineComplete);
                observer.active_ticks = 0;
            } else if observer.active_ticks >= 12 {
                sounds.write(SoundEvent::ManualMineTick);
                observer.active_ticks = 0;
            }
        } else {
            observer.active_ticks = 0;
        }
    } else if previous.is_some() && current.is_none() {
        sounds.write(SoundEvent::ManualMineComplete);
        observer.active_ticks = 0;
    } else if current.is_none() {
        observer.active_ticks = 0;
    }

    observer.previous = current;
}

pub(crate) fn observe_crafting_audio(
    sim: Res<SimResource>,
    mut observer: ResMut<CraftingAudioObserver>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let queue = sim.sim.crafting_queue();
    let current_front = queue.entries.front().copied();
    let current_len = queue.entries.len();

    if let Some(previous_front) = observer.previous_front
        && (current_len < observer.previous_len || current_front != Some(previous_front))
    {
        sounds.write(SoundEvent::CraftComplete);
    }

    observer.previous_front = current_front;
    observer.previous_len = current_len;
}

pub(crate) fn observe_research_audio(
    sim: Res<SimResource>,
    mut observer: ResMut<ResearchAudioObserver>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let unlocked = sim
        .sim
        .catalog()
        .technologies
        .iter()
        .filter(|technology| sim.sim.is_technology_unlocked(technology.id))
        .map(|technology| technology.id)
        .collect::<HashSet<_>>();

    if observer.initialized && unlocked.iter().any(|id| !observer.unlocked.contains(id)) {
        sounds.write(SoundEvent::ResearchComplete);
    }

    observer.initialized = true;
    observer.unlocked = unlocked;
}

pub(crate) fn sync_machine_audio_loops(
    mut commands: Commands,
    sim: Res<SimResource>,
    visible: Res<VisibleEntityIds>,
    assets: Res<AudioAssets>,
    settings: Res<AudioSettings>,
    mut loops: ResMut<MachineAudioLoops>,
) {
    let effective_volume = settings.effective_volume();
    if effective_volume <= 0.0 {
        despawn_all_loops(&mut commands, &mut loops);
        return;
    }

    let mut candidates = visible
        .ids
        .iter()
        .filter_map(|entity_id| machine_loop_candidate(&sim.sim, *entity_id))
        .collect::<Vec<_>>();

    let (player_x, player_y) = sim.sim.player().position_tiles();
    candidates.sort_by(|a, b| {
        a.distance_squared(player_x, player_y)
            .total_cmp(&b.distance_squared(player_x, player_y))
            .then_with(|| a.entity_id.raw().cmp(&b.entity_id.raw()))
    });
    candidates.truncate(MAX_MACHINE_LOOPS);

    let target_ids = candidates
        .iter()
        .map(|candidate| candidate.entity_id)
        .collect::<HashSet<_>>();
    loops.by_entity.retain(|entity_id, audio_entity| {
        if target_ids.contains(entity_id) {
            true
        } else {
            commands.entity(*audio_entity).despawn();
            false
        }
    });

    for candidate in candidates {
        if loops.by_entity.contains_key(&candidate.entity_id) {
            continue;
        }
        let handle = match candidate.loop_kind {
            MachineLoopKind::Burner => assets.machine_burner_loop.clone(),
            MachineLoopKind::Electric => assets.machine_electric_loop.clone(),
        };
        let Some(handle) = handle else {
            continue;
        };
        let audio_entity = commands
            .spawn((
                AudioPlayer::new(handle),
                PlaybackSettings::LOOP
                    .with_volume(Volume::Linear(effective_volume * MACHINE_LOOP_GAIN)),
                Transform::from_translation(candidate.translation),
                GlobalTransform::default(),
                MachineLoopAudio {
                    entity_id: candidate.entity_id,
                },
            ))
            .id();
        loops.by_entity.insert(candidate.entity_id, audio_entity);
    }
}

fn despawn_all_loops(commands: &mut Commands, loops: &mut MachineAudioLoops) {
    for (_, entity) in loops.by_entity.drain() {
        commands.entity(entity).despawn();
    }
}

fn machine_loop_candidate(
    sim: &factory_sim::Simulation,
    entity_id: EntityId,
) -> Option<LoopCandidate> {
    if sim.machine_status_for_entity(entity_id) != Some(MachineStatus::Working) {
        return None;
    }
    let placed = sim.entities().placed_entity(entity_id)?;
    let prototype = sim.catalog().entity(placed.prototype_id)?;
    let loop_kind = match prototype.entity_kind {
        EntityKind::MiningDrill | EntityKind::Furnace | EntityKind::Boiler => {
            MachineLoopKind::Burner
        }
        EntityKind::AssemblingMachine
        | EntityKind::Lab
        | EntityKind::SteamEngine
        | EntityKind::OffshorePump => MachineLoopKind::Electric,
        _ => return None,
    };
    Some(LoopCandidate {
        entity_id,
        center_x: placed.footprint.x as f32 + placed.footprint.width as f32 * 0.5,
        center_y: placed.footprint.y as f32 + placed.footprint.height as f32 * 0.5,
        translation: entity_translation(&placed.footprint, 0.0),
        loop_kind,
    })
}

fn sound_handle(assets: &AudioAssets, event: SoundEvent) -> Option<&Handle<AudioSource>> {
    match event {
        SoundEvent::UiClick => assets.ui_click.as_ref(),
        SoundEvent::Place => assets.place.as_ref(),
        SoundEvent::PlaceError => assets.place_error.as_ref(),
        SoundEvent::ManualMineTick => assets.manual_mine_tick.as_ref(),
        SoundEvent::ManualMineComplete => assets.manual_mine_complete.as_ref(),
        SoundEvent::CraftComplete => assets.craft_complete.as_ref(),
        SoundEvent::ResearchComplete => assets.research_complete.as_ref(),
    }
}

fn one_shot_gain(event: SoundEvent) -> f32 {
    match event {
        SoundEvent::UiClick => 0.35,
        SoundEvent::Place => 0.8,
        SoundEvent::PlaceError => 0.55,
        SoundEvent::ManualMineTick => 0.30,
        SoundEvent::ManualMineComplete => 0.75,
        SoundEvent::CraftComplete => 0.75,
        SoundEvent::ResearchComplete => 0.85,
    }
}

fn sound_cooldown_ticks(event: SoundEvent) -> u64 {
    match event {
        SoundEvent::ManualMineTick => 12,
        SoundEvent::PlaceError => 4,
        SoundEvent::UiClick => 1,
        _ => 0,
    }
}

#[derive(Clone, Copy)]
enum MachineLoopKind {
    Burner,
    Electric,
}

struct LoopCandidate {
    entity_id: EntityId,
    center_x: f32,
    center_y: f32,
    translation: Vec3,
    loop_kind: MachineLoopKind,
}

impl LoopCandidate {
    fn distance_squared(&self, player_x: f32, player_y: f32) -> f32 {
        let dx = self.center_x - player_x;
        let dy = self.center_y - player_y;
        dx * dx + dy * dy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn audio_settings_defaults_are_valid() {
        let settings = AudioSettings::default();
        assert!(!settings.muted);
        assert_eq!(settings.volume, 0.65);
    }

    #[test]
    fn audio_settings_volume_is_clamped() {
        let mut settings = AudioSettings::default();
        settings.adjust_volume_steps(-20);
        assert_eq!(settings.volume, 0.0);
        settings.adjust_volume_steps(20);
        assert_eq!(settings.volume, 1.0);
    }

    #[test]
    fn settings_file_round_trip() {
        let root = std::env::temp_dir().join(format!(
            "factory-audio-settings-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after epoch")
                .as_nanos()
        ));
        let path = root.join("settings.ron");
        let file = AudioSettingsFile {
            muted: true,
            volume: 0.42,
        };

        write_audio_settings_file(&path, &file).expect("settings file should write");
        let loaded = read_audio_settings_file(&path).expect("settings file should load");

        assert_eq!(loaded, file);
        let _ = fs::remove_dir_all(root);
    }
}
