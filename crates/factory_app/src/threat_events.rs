//! Shared cursor over the simulation's bounded threat-event log.

use factory_sim::{Simulation, ThreatEvent};

/// Tracks which threat events an observer has already consumed. The threat
/// alert UI and the enemy-warning audio both poll the same log; this owns
/// the initialize/reload/cursor dance so neither duplicates it.
#[derive(Default)]
pub(crate) struct ThreatEventCursor {
    initialized: bool,
    reload_token: u64,
    cursor: u64,
}

pub(crate) struct ThreatEventPoll {
    /// The observed world changed: first poll, or a save was loaded. Derived
    /// state built from earlier events should be dropped; the events already
    /// in the log are skipped rather than replayed.
    pub(crate) reset: bool,
    /// Events emitted since the previous poll, oldest first.
    pub(crate) events: Vec<ThreatEvent>,
}

impl ThreatEventCursor {
    pub(crate) fn poll_new(&mut self, sim: &Simulation, reload_token: u64) -> ThreatEventPoll {
        if !self.initialized || self.reload_token != reload_token {
            self.initialized = true;
            self.reload_token = reload_token;
            self.cursor = sim.latest_threat_sequence();
            return ThreatEventPoll {
                reset: true,
                events: Vec::new(),
            };
        }

        let events = sim.threat_events_after(self.cursor);
        if let Some(event) = events.last() {
            self.cursor = event.sequence;
        }
        ThreatEventPoll {
            reset: false,
            events,
        }
    }
}
