use super::*;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub(super) struct StatisticsSubsystem {
    pub(super) items: ItemStatistics,
    pub(super) fluids: FluidStatistics,
    pub(super) power: PowerStatistics,
    /// Total successfully completed launches across the lifetime of the world.
    pub(super) rockets_launched: u64,
}
