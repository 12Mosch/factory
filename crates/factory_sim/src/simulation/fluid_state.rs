use super::*;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Hash, Serialize)]
pub(super) struct FluidSubsystem {
    pub(super) networks: Vec<FluidNetworkSnapshot>,
}
