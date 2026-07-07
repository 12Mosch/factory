use crate::entities::store::{EntityStore, for_each_entity_state_map};
use crate::entities::{Direction, EntityFootprint};
use crate::ids::EntityId;
use factory_data::EntityPrototypeId;

macro_rules! define_entity_reservation {
    ($($field:ident : $ty:ty => $kind:tt),* $(,)?) => {
        /// Placement request for a new entity: its footprint plus the initial
        /// per-kind state entries to insert, one optional slot per state map.
        pub(crate) struct EntityReservation {
            pub(crate) prototype_id: EntityPrototypeId,
            pub(crate) x: i32,
            pub(crate) y: i32,
            pub(crate) direction: Direction,
            pub(crate) footprint: EntityFootprint,
            $(pub(crate) $field: Option<$ty>,)*
        }

        impl EntityReservation {
            /// Reservation without any per-kind state.
            pub(crate) fn new(
                prototype_id: EntityPrototypeId,
                x: i32,
                y: i32,
                direction: Direction,
                footprint: EntityFootprint,
            ) -> Self {
                Self {
                    prototype_id,
                    x,
                    y,
                    direction,
                    footprint,
                    $($field: None,)*
                }
            }
        }

        impl EntityStore {
            /// Inserts every reserved state entry for the newly allocated id.
            pub(crate) fn insert_reserved_states(
                &mut self,
                id: EntityId,
                reservation: EntityReservation,
            ) {
                $(
                    if let Some(state) = reservation.$field {
                        self.$field.insert(id, state);
                    }
                )*
            }
        }
    };
}
for_each_entity_state_map!(define_entity_reservation);
