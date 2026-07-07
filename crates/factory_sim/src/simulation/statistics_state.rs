use super::*;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub(super) struct StatisticsSubsystem {
    pub(super) items: ItemStatistics,
    pub(super) fluids: FluidStatistics,
    pub(super) power: PowerStatistics,
}
