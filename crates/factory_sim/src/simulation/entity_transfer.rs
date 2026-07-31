//! Player-driven item transfers between inventories and machine-specific slots.
//!
//! The transfer planner is shared by the entity-specific modules so validation
//! always completes before either endpoint is mutated.

use super::*;

mod assemblers;
mod containers;
mod furnaces;
mod inserters;
mod mining_drills;
mod modules;
mod power_entities;
mod roboports;
mod rolling_stock;
mod routing;
mod transfer;

pub use assemblers::*;
pub use containers::*;
pub use furnaces::*;
pub use inserters::*;
pub use mining_drills::*;
pub use power_entities::*;
pub use roboports::*;
pub use rolling_stock::*;
pub use routing::{transfer_container_slot, transfer_rolling_stock_slot};
pub use transfer::TransferOutcome;

use modules::*;
use transfer::*;
