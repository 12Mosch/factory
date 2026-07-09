use serde::{Deserialize, Serialize};

/// Pumpjacks hold no per-entity progress; production happens at a fixed rate
/// into the entity's output fluid box whenever it is powered and placed over
/// its resource. The state exists so the entity registry can identify the
/// machine kind.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct PumpjackState;
