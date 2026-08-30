use crate::save_load::PresentationReloadToken;
use bevy::audio::{AudioSink, AudioSinkPlayback, SpatialAudioSink, SpatialScale, Volume};
use bevy::prelude::*;
use factory_data::EntityKind;
use factory_sim::{
    EntityId, MachineStatus, ManualMiningProgress, RocketLaunchPhase, ThreatEventKind,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::rendering::resources::VisibleEntityIds;
use crate::rendering::transforms::entity_translation;
use crate::resources::SimResource;
use crate::save_load::SaveLoadConfig;
use crate::threat_events::ThreatEventCursor;

const DEFAULT_VOLUME: f32 = 0.65;
const VOLUME_STEP: f32 = 0.10;
const MAX_MACHINE_LOOPS: usize = 32;
const MACHINE_LOOP_GAIN: f32 = 0.18;
const ROCKET_AUDIO_DISTANCE_TILES: f32 = 8.0;

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SoundEvent {
    UiClick,
    AudioTest,
    Place,
    PlaceError,
    ManualMineTick,
    ManualMineComplete,
    CraftComplete,
    ResearchComplete,
    EnemyWarning,
    RocketSeal { entity_id: EntityId },
    RocketLaunch { entity_id: EntityId },
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
    pub enemy_warning: Option<Handle<AudioSource>>,
    pub rocket_seal: Option<Handle<AudioSource>>,
    pub rocket_launch: Option<Handle<AudioSource>>,
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
    initialized: bool,
    sim_replacement_revision: u64,
    previous_completed_jobs: u64,
}

impl CraftingAudioObserver {
    fn observe(&mut self, sim_replacement_revision: u64, completed_jobs: u64) -> bool {
        let completed_in_current_world = self.initialized
            && self.sim_replacement_revision == sim_replacement_revision
            && self.previous_completed_jobs != completed_jobs;

        self.initialized = true;
        self.sim_replacement_revision = sim_replacement_revision;
        self.previous_completed_jobs = completed_jobs;
        completed_in_current_world
    }
}

#[derive(Resource, Default)]
pub struct ResearchAudioObserver {
    initialized: bool,
    completed_levels: Vec<u32>,
}

#[derive(Resource, Default)]
pub struct ThreatAudioObserver {
    cursor: ThreatEventCursor,
}

#[derive(Resource, Default)]
pub struct RocketLaunchAudioObserver {
    initialized: bool,
    reload_token: u64,
    phases: BTreeMap<EntityId, RocketAudioPhase>,
}

#[derive(Resource, Default)]
pub struct AudioSettingsPersistenceState {
    last_saved: Option<AudioSettingsFile>,
}

#[derive(Component)]
pub struct MachineLoopAudio {
    pub entity_id: EntityId,
}

#[derive(Component)]
pub struct SoundEffectAudio {
    gain: f32,
}

#[derive(Component)]
pub struct RocketLaunchAudioPlayback {
    reload_token: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RocketAudioPhase {
    Idle,
    Sealing,
    Rising,
}

impl From<RocketLaunchPhase> for RocketAudioPhase {
    fn from(phase: RocketLaunchPhase) -> Self {
        match phase {
            RocketLaunchPhase::Idle => Self::Idle,
            RocketLaunchPhase::Sealed { .. } => Self::Sealing,
            RocketLaunchPhase::Rising { .. } => Self::Rising,
        }
    }
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
    assets.enemy_warning = Some(asset_server.load("audio/enemy_warning.wav"));
    assets.rocket_seal = Some(asset_server.load("audio/rocket_seal.wav"));
    assets.rocket_launch = Some(asset_server.load("audio/rocket_launch.wav"));
}

pub(crate) fn initialize_rocket_launch_audio(
    sim: Res<SimResource>,
    reload: Option<Res<PresentationReloadToken>>,
    mut observer: ResMut<RocketLaunchAudioObserver>,
) {
    let reload_token = reload.as_deref().map_or(0, |token| token.value);
    observer.reset(&sim.read(), reload_token);
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
    reload: Option<Res<PresentationReloadToken>>,
    mut dedupe: ResMut<AudioEventDedupe>,
) {
    let effective_volume = settings.effective_volume();
    if effective_volume <= 0.0 {
        events.clear();
        return;
    }

    let tick = sim.is_initialized().then(|| sim.read().tick_count());
    for event in events.read() {
        let cooldown = sound_cooldown_ticks(*event);
        if tick.is_some_and(|tick| {
            dedupe
                .last_played_tick
                .get(event)
                .is_some_and(|last_tick| cooldown > 0 && tick.saturating_sub(*last_tick) < cooldown)
        }) {
            continue;
        }
        let Some(handle) = sound_handle(&assets, *event).cloned() else {
            continue;
        };
        if cooldown > 0
            && let Some(tick) = tick
        {
            dedupe.last_played_tick.insert(*event, tick);
        }
        let gain = one_shot_gain(*event);
        let translation = sim
            .is_initialized()
            .then(|| spatial_sound_translation(&sim.read(), *event))
            .flatten();
        let mut playback =
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(effective_volume * gain));
        if translation.is_some() {
            playback = playback
                .with_spatial(true)
                .with_spatial_scale(SpatialScale::new_2d(
                    1.0 / (crate::constants::TILE_SIZE * ROCKET_AUDIO_DISTANCE_TILES),
                ));
        }
        let mut audio_entity = commands.spawn((
            AudioPlayer::new(handle),
            playback,
            SoundEffectAudio { gain },
        ));
        if let Some(translation) = translation {
            audio_entity.insert((
                Transform::from_translation(translation),
                GlobalTransform::default(),
                RocketLaunchAudioPlayback {
                    reload_token: reload.as_deref().map_or(0, |token| token.value),
                },
            ));
        }
    }
}

pub(crate) fn apply_audio_settings_to_sinks(
    settings: Res<AudioSettings>,
    mut sinks: Query<(
        &mut AudioSink,
        Option<&MachineLoopAudio>,
        Option<&SoundEffectAudio>,
    )>,
    mut spatial_sinks: Query<(&mut SpatialAudioSink, &SoundEffectAudio)>,
) {
    if !settings.is_changed() {
        return;
    }

    let effective_volume = settings.effective_volume();
    for (mut sink, loop_marker, sound_effect) in &mut sinks {
        let gain = if loop_marker.is_some() {
            MACHINE_LOOP_GAIN
        } else {
            sound_effect.map_or(1.0, |effect| effect.gain)
        };
        sink.set_volume(Volume::Linear(effective_volume * gain));
    }
    for (mut sink, sound_effect) in &mut spatial_sinks {
        sink.set_volume(Volume::Linear(effective_volume * sound_effect.gain));
    }
}

pub(crate) fn observe_manual_mining_audio(
    sim: Res<SimResource>,
    mut observer: ResMut<ManualMiningAudioObserver>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let current = sim.read().manual_mining_progress();
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
    let sim_replacement_revision = sim.replacement_revision();
    let sim = sim.read();
    if observer.observe(
        sim_replacement_revision,
        sim.crafting_queue().completed_jobs,
    ) {
        sounds.write(SoundEvent::CraftComplete);
    }
}

pub(crate) fn observe_research_audio(
    sim: Res<SimResource>,
    mut observer: ResMut<ResearchAudioObserver>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let completed_levels = sim
        .read()
        .catalog()
        .technologies
        .iter()
        .map(|technology| sim.read().technology_level(technology.id).unwrap_or(0))
        .collect::<Vec<_>>();

    if observer.initialized
        && completed_levels
            .iter()
            .zip(&observer.completed_levels)
            .any(|(current, previous)| current > previous)
    {
        sounds.write(SoundEvent::ResearchComplete);
    }

    observer.initialized = true;
    observer.completed_levels = completed_levels;
}

pub(crate) fn observe_threat_audio(
    sim: Res<SimResource>,
    reload: Option<Res<PresentationReloadToken>>,
    mut observer: ResMut<ThreatAudioObserver>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let reload_token = reload.as_deref().map_or(0, |token| token.value);
    let poll = observer.cursor.poll_new(&sim.read(), reload_token);
    for event in &poll.events {
        if matches!(
            event.kind,
            ThreatEventKind::RaidLaunched | ThreatEventKind::StructureUnderAttack
        ) {
            sounds.write(SoundEvent::EnemyWarning);
        }
    }
}

pub(crate) fn observe_rocket_launch_audio(
    sim: Res<SimResource>,
    reload: Option<Res<PresentationReloadToken>>,
    mut observer: ResMut<RocketLaunchAudioObserver>,
    mut sounds: MessageWriter<SoundEvent>,
) {
    let reload_token = reload.as_deref().map_or(0, |token| token.value);
    for sound in observer.poll(&sim.read(), reload_token) {
        sounds.write(sound);
    }
}

pub(crate) fn cleanup_reloaded_rocket_launch_audio(
    mut commands: Commands,
    reload: Option<Res<PresentationReloadToken>>,
    playback: Query<(Entity, &RocketLaunchAudioPlayback)>,
) {
    let reload_token = reload.as_deref().map_or(0, |token| token.value);
    for (entity, playback) in &playback {
        if playback.reload_token != reload_token {
            commands.entity(entity).despawn();
        }
    }
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
        .filter_map(|entity_id| machine_loop_candidate(&sim.read(), *entity_id))
        .collect::<Vec<_>>();

    let (player_x, player_y) = sim.read().player().position_tiles();
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
        // A reactor burns fuel cells, so it shares the burner loop.
        EntityKind::MiningDrill
        | EntityKind::Furnace
        | EntityKind::Boiler
        | EntityKind::NuclearReactor => MachineLoopKind::Burner,
        EntityKind::AssemblingMachine
        | EntityKind::RocketSilo
        | EntityKind::Lab
        | EntityKind::SteamEngine
        | EntityKind::OffshorePump
        | EntityKind::Pump
        | EntityKind::HeatExchanger => MachineLoopKind::Electric,
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
        SoundEvent::AudioTest => assets.craft_complete.as_ref(),
        SoundEvent::Place => assets.place.as_ref(),
        SoundEvent::PlaceError => assets.place_error.as_ref(),
        SoundEvent::ManualMineTick => assets.manual_mine_tick.as_ref(),
        SoundEvent::ManualMineComplete => assets.manual_mine_complete.as_ref(),
        SoundEvent::CraftComplete => assets.craft_complete.as_ref(),
        SoundEvent::ResearchComplete => assets.research_complete.as_ref(),
        SoundEvent::EnemyWarning => assets.enemy_warning.as_ref(),
        SoundEvent::RocketSeal { .. } => assets.rocket_seal.as_ref(),
        SoundEvent::RocketLaunch { .. } => assets.rocket_launch.as_ref(),
    }
}

fn one_shot_gain(event: SoundEvent) -> f32 {
    match event {
        SoundEvent::UiClick => 0.35,
        SoundEvent::AudioTest => 0.75,
        SoundEvent::Place => 0.8,
        SoundEvent::PlaceError => 0.55,
        SoundEvent::ManualMineTick => 0.30,
        SoundEvent::ManualMineComplete => 0.75,
        SoundEvent::CraftComplete => 0.75,
        SoundEvent::ResearchComplete => 0.85,
        SoundEvent::EnemyWarning => 0.9,
        SoundEvent::RocketSeal { .. } => 0.75,
        SoundEvent::RocketLaunch { .. } => 1.0,
    }
}

fn sound_cooldown_ticks(event: SoundEvent) -> u64 {
    match event {
        SoundEvent::ManualMineTick => 12,
        SoundEvent::PlaceError => 4,
        SoundEvent::UiClick => 1,
        SoundEvent::EnemyWarning => 600,
        SoundEvent::RocketSeal { .. }
        | SoundEvent::RocketLaunch { .. }
        | SoundEvent::AudioTest
        | SoundEvent::Place
        | SoundEvent::ManualMineComplete
        | SoundEvent::CraftComplete
        | SoundEvent::ResearchComplete => 0,
    }
}

fn spatial_sound_translation(sim: &factory_sim::Simulation, event: SoundEvent) -> Option<Vec3> {
    let entity_id = match event {
        SoundEvent::RocketSeal { entity_id } | SoundEvent::RocketLaunch { entity_id } => entity_id,
        _ => return None,
    };
    let placed = sim.entities().placed_entity(entity_id)?;
    Some(entity_translation(&placed.footprint, 0.0))
}

impl RocketLaunchAudioObserver {
    fn reset(&mut self, sim: &factory_sim::Simulation, reload_token: u64) {
        self.initialized = true;
        self.reload_token = reload_token;
        self.phases = rocket_audio_phases(sim);
    }

    fn poll(&mut self, sim: &factory_sim::Simulation, reload_token: u64) -> Vec<SoundEvent> {
        if !self.initialized || self.reload_token != reload_token {
            self.reset(sim, reload_token);
            return Vec::new();
        }

        let current = rocket_audio_phases(sim);
        let mut sounds = Vec::new();
        for (&entity_id, &phase) in &current {
            match (self.phases.get(&entity_id).copied(), phase) {
                (Some(previous), current) if previous == current => {}
                (Some(RocketAudioPhase::Idle), RocketAudioPhase::Rising) => {
                    // `Update` runs after any fixed-step catch-up. Preserve both
                    // transition sounds if the seal phase elapsed in one frame.
                    sounds.push(SoundEvent::RocketSeal { entity_id });
                    sounds.push(SoundEvent::RocketLaunch { entity_id });
                }
                (Some(RocketAudioPhase::Sealing), RocketAudioPhase::Idle) => {
                    // The entire rise phase elapsed during fixed-step catch-up.
                    sounds.push(SoundEvent::RocketLaunch { entity_id });
                }
                (_, RocketAudioPhase::Sealing) => {
                    sounds.push(SoundEvent::RocketSeal { entity_id });
                }
                (_, RocketAudioPhase::Rising) => {
                    sounds.push(SoundEvent::RocketLaunch { entity_id });
                }
                (_, RocketAudioPhase::Idle) => {}
            }
        }
        self.phases = current;
        sounds
    }
}

fn rocket_audio_phases(sim: &factory_sim::Simulation) -> BTreeMap<EntityId, RocketAudioPhase> {
    factory_sim::entity_access::rocket_silo_launch_phases(sim)
        .map(|(entity_id, phase)| (entity_id, phase.into()))
        .collect()
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
    fn crafting_audio_does_not_treat_world_replacement_as_completion() {
        let mut observer = CraftingAudioObserver::default();

        assert!(!observer.observe(0, 3));
        assert!(observer.observe(0, 4));
        assert!(!observer.observe(1, 0));
        assert!(observer.observe(1, 1));
    }

    #[test]
    fn rocket_audio_tracks_phase_transitions_and_resets_on_reload() {
        let mut sim = factory_sim::Simulation::new_rocket_launch_fixture();
        let silo_id = factory_sim::entity_access::rocket_silo_launch_phases(&sim)
            .next()
            .expect("launch fixture should contain a silo")
            .0;
        let mut observer = RocketLaunchAudioObserver::default();
        observer.reset(&sim, 0);

        sim.tick();
        assert_eq!(
            observer.poll(&sim, 0),
            vec![SoundEvent::RocketSeal { entity_id: silo_id }]
        );
        for _ in 0..60 {
            sim.tick();
        }
        assert_eq!(
            observer.poll(&sim, 0),
            vec![SoundEvent::RocketLaunch { entity_id: silo_id }]
        );

        let loaded = factory_sim::load_from_bytes(
            &factory_sim::save_to_bytes(&sim).expect("mid-launch simulation should save"),
        )
        .expect("mid-launch simulation should load");
        assert!(
            observer.poll(&loaded, 1).is_empty(),
            "reloading a running launch must not replay its transition sound"
        );
    }

    #[test]
    fn rocket_audio_preserves_transitions_across_fixed_step_catch_up() {
        let mut sim = factory_sim::Simulation::new_rocket_launch_fixture();
        let silo_id = factory_sim::entity_access::rocket_silo_launch_phases(&sim)
            .next()
            .expect("launch fixture should contain a silo")
            .0;
        let mut observer = RocketLaunchAudioObserver::default();
        observer.reset(&sim, 0);

        for _ in 0..=60 {
            sim.tick();
        }

        assert_eq!(
            observer.poll(&sim, 0),
            vec![
                SoundEvent::RocketSeal { entity_id: silo_id },
                SoundEvent::RocketLaunch { entity_id: silo_id },
            ]
        );
    }

    #[test]
    fn rocket_audio_uses_the_silo_world_position() {
        let sim = factory_sim::Simulation::new_rocket_launch_fixture();
        let silo_id = factory_sim::entity_access::rocket_silo_launch_phases(&sim)
            .next()
            .expect("launch fixture should contain a silo")
            .0;
        let footprint = &sim
            .entities()
            .placed_entity(silo_id)
            .expect("launch fixture silo should be placed")
            .footprint;

        assert_eq!(
            spatial_sound_translation(&sim, SoundEvent::RocketLaunch { entity_id: silo_id }),
            Some(entity_translation(footprint, 0.0))
        );
        assert!(spatial_sound_translation(&sim, SoundEvent::CraftComplete).is_none());
    }

    #[test]
    fn world_reload_despawns_old_rocket_audio_playback() {
        let mut app = App::new();
        app.insert_resource(PresentationReloadToken { value: 2 })
            .add_systems(Update, cleanup_reloaded_rocket_launch_audio);
        app.world_mut()
            .spawn(RocketLaunchAudioPlayback { reload_token: 1 });

        app.update();

        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<RocketLaunchAudioPlayback>>()
                .iter(app.world())
                .count(),
            0
        );
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
