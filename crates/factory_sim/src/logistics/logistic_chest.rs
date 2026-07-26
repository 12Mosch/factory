//! Per-chest logistic configuration: what a chest asks its network for, and
//! what it will accept.
//!
//! The five logistic chest variants are one [`factory_data::EntityKind::Chest`]
//! with a [`factory_data::LogisticChestPrototype`] rather than five kinds, so
//! everything that already works on a chest — the inventory, the container
//! window, inserter transfers, circuit contents — keeps working untouched. The
//! only durable per-entity addition is the row list below, and only the modes
//! that declare rows carry a non-empty one.

use crate::prototypes::ItemId;
use serde::{Deserialize, Serialize};

/// One configured row of a logistic chest.
///
/// What the row means depends on the chest's mode, which is prototype data and
/// therefore never disagrees between two chests of the same kind:
///
/// * on a requester or buffer chest it is "keep `count` of `item` here";
/// * on a storage chest it is a filter, and `count` is unused and always zero.
///
/// An unset `item` is an empty row rather than a special case: it asks for
/// nothing and filters nothing.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct LogisticRequest {
    pub item: Option<ItemId>,
    pub count: u32,
}

impl LogisticRequest {
    /// The row's demand, which is nothing at all unless both halves are set.
    pub fn demand(self) -> Option<(ItemId, u32)> {
        let item = self.item?;
        (self.count > 0).then_some((item, self.count))
    }
}

/// Durable logistic configuration of one chest.
///
/// Present on every chest whose prototype declares a logistic role, including
/// the provider modes that configure nothing: the entry is what marks the chest
/// as part of the network, so the index never has to consult the prototype to
/// find out which chests to walk.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub struct LogisticChestState {
    /// One entry per row the prototype declares; never resized at runtime.
    pub requests: Vec<LogisticRequest>,
}

impl LogisticChestState {
    pub(crate) fn with_slot_count(slot_count: usize) -> Self {
        Self {
            requests: vec![LogisticRequest::default(); slot_count],
        }
    }

    /// Item a storage chest is filtered to, if any. Reading it through a named
    /// accessor keeps "the filter lives in row zero" in one place.
    pub fn storage_filter(&self) -> Option<ItemId> {
        self.requests.first().and_then(|request| request.item)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogisticChestError {
    MissingEntity(crate::ids::EntityId),
    /// The entity is not a chest with a logistic role.
    NotLogisticChest(crate::ids::EntityId),
    InvalidSlot {
        slot_index: usize,
    },
    UnknownItem(ItemId),
    /// A provider chest supplies what it holds and configures nothing, and a
    /// storage chest's single row is a filter rather than an amount.
    ModeTakesNoAmount,
}
