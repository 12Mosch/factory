use serde::{Deserialize, Serialize};

/// Durable powered-work and sweep state for a placed radar.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct RadarState {
    pub(crate) nearby_scan_progress_ticks: u32,
    pub(crate) far_scan_progress_ticks: u32,
    pub(crate) far_scan_cursor: u64,
    pub(crate) far_scan_complete: bool,
}

impl RadarState {
    pub const fn nearby_scan_progress_ticks(&self) -> u32 {
        self.nearby_scan_progress_ticks
    }

    pub const fn far_scan_progress_ticks(&self) -> u32 {
        self.far_scan_progress_ticks
    }

    pub const fn far_scan_cursor(&self) -> u64 {
        self.far_scan_cursor
    }

    pub const fn far_scan_complete(&self) -> bool {
        self.far_scan_complete
    }
}

pub(crate) fn far_scan_candidate_count(nearby_radius: u16, far_radius: u16) -> u64 {
    let nearby_width = u64::from(nearby_radius) * 2 + 1;
    let far_width = u64::from(far_radius) * 2 + 1;
    far_width * far_width - nearby_width * nearby_width
}
