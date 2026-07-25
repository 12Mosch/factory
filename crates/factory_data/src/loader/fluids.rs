use std::collections::HashMap;

use crate::ids::FluidId;
use crate::model::FluidPrototype;
use crate::raw::RawFluidPrototype;

pub(super) fn load_fluids(
    fluids: Vec<RawFluidPrototype>,
) -> (Vec<FluidPrototype>, HashMap<String, FluidId>) {
    let mut fluid_ids_by_name = HashMap::with_capacity(fluids.len());
    let fluids = fluids
        .into_iter()
        .map(|fluid| {
            let id = FluidId::new(fluid.id);
            fluid_ids_by_name.insert(fluid.name.clone(), id);
            FluidPrototype {
                id,
                name: fluid.name,
            }
        })
        .collect();

    (fluids, fluid_ids_by_name)
}
