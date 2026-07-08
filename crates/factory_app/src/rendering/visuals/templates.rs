use factory_data::EntityKind;
use factory_sim::Direction;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum VisualTemplate {
    Entity {
        kind: EntityKind,
        direction: Direction,
    },
    BeltItem,
    Resource,
}
