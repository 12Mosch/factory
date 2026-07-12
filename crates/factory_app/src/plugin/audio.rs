use bevy::prelude::*;

use super::{AppSet, InGameSet};
use crate::audio::{
    AudioAssets, AudioEventDedupe, AudioSettings, AudioSettingsPersistenceState,
    AudioSettingsWindowState, CraftingAudioObserver, MachineAudioLoops, ManualMiningAudioObserver,
    ResearchAudioObserver, SoundEvent, ThreatAudioObserver, apply_audio_settings_to_sinks,
    load_audio_assets, load_persisted_audio_settings, observe_crafting_audio,
    observe_manual_mining_audio, observe_research_audio, observe_threat_audio, play_sound_events,
    save_audio_settings_if_changed, sync_machine_audio_loops,
};
use crate::ui::audio_settings::{handle_audio_settings_buttons, sync_audio_settings_window};

/// Sound-effect playback, machine audio loops, and audio settings.
pub(super) struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudioSettings>()
            .init_resource::<AudioSettingsWindowState>()
            .init_resource::<AudioAssets>()
            .init_resource::<MachineAudioLoops>()
            .init_resource::<AudioEventDedupe>()
            .init_resource::<ManualMiningAudioObserver>()
            .init_resource::<CraftingAudioObserver>()
            .init_resource::<ResearchAudioObserver>()
            .init_resource::<ThreatAudioObserver>()
            .init_resource::<AudioSettingsPersistenceState>()
            .add_message::<SoundEvent>()
            .add_systems(Startup, (load_persisted_audio_settings, load_audio_assets))
            .add_systems(
                FixedUpdate,
                (
                    observe_manual_mining_audio,
                    observe_crafting_audio,
                    observe_research_audio,
                    observe_threat_audio,
                )
                    .in_set(AppSet::PostTick),
            )
            .add_systems(
                Update,
                (
                    handle_audio_settings_buttons.in_set(AppSet::UiInteraction),
                    save_audio_settings_if_changed,
                    sync_audio_settings_window.in_set(InGameSet),
                    apply_audio_settings_to_sinks,
                )
                    .chain()
                    .before(AppSet::MapTexture),
            )
            .add_systems(
                Update,
                (
                    sync_machine_audio_loops
                        .after(AppSet::VisibleEntities)
                        .in_set(InGameSet),
                    play_sound_events.after(AppSet::UiInteraction),
                ),
            );
    }
}
