//! Signal naming and enumeration shared by every circuit editor.

use factory_data::{PrototypeCatalog, VirtualSignalKind};
use factory_sim::SignalId;

use crate::placement::build::display_name;
use crate::utils::compact_item_name;

/// Every signal a player can pick, grouped so the picker can render headed
/// sections without re-deriving the grouping.
pub(crate) struct SignalCatalog {
    pub(crate) items: Vec<SignalId>,
    pub(crate) fluids: Vec<SignalId>,
    pub(crate) virtuals: Vec<SignalId>,
}

impl SignalCatalog {
    pub(crate) fn from_catalog(catalog: &PrototypeCatalog) -> Self {
        Self {
            items: catalog
                .items
                .iter()
                .map(|item| SignalId::Item(item.id))
                .collect(),
            fluids: catalog
                .fluids
                .iter()
                .map(|fluid| SignalId::Fluid(fluid.id))
                .collect(),
            virtuals: catalog
                .virtual_signals
                .iter()
                .map(|signal| SignalId::Virtual(signal.id))
                .collect(),
        }
    }
}

/// Full, human-readable name of a signal.
pub(crate) fn signal_display_name(catalog: &PrototypeCatalog, signal: SignalId) -> String {
    match signal {
        SignalId::Item(item_id) => catalog
            .item(item_id)
            .map(|item| display_name(&item.name))
            .unwrap_or_else(|| "Unknown item".to_string()),
        SignalId::Fluid(fluid_id) => catalog
            .fluid(fluid_id)
            .map(|fluid| display_name(&fluid.name))
            .unwrap_or_else(|| "Unknown fluid".to_string()),
        SignalId::Virtual(virtual_id) => catalog
            .virtual_signal(virtual_id)
            .map(|signal| display_name(&signal.name))
            .unwrap_or_else(|| "Unknown signal".to_string()),
    }
}

/// Short label for a signal button, sized for a compact grid cell.
///
/// Wildcards keep a readable word because their meaning is not recoverable
/// from initials, while ordinary signals collapse to their initials the same
/// way build-menu entries do.
pub(crate) fn signal_short_label(catalog: &PrototypeCatalog, signal: SignalId) -> String {
    if let SignalId::Virtual(virtual_id) = signal
        && let Some(prototype) = catalog.virtual_signal(virtual_id)
    {
        return match prototype.kind {
            VirtualSignalKind::Each => "EACH".to_string(),
            VirtualSignalKind::Anything => "ANY".to_string(),
            VirtualSignalKind::Everything => "ALL".to_string(),
            VirtualSignalKind::Concrete => prototype
                .name
                .strip_prefix("signal_")
                .unwrap_or(&prototype.name)
                .to_uppercase(),
        };
    }
    let name = match signal {
        SignalId::Item(item_id) => catalog.item(item_id).map(|item| item.name.as_str()),
        SignalId::Fluid(fluid_id) => catalog.fluid(fluid_id).map(|fluid| fluid.name.as_str()),
        SignalId::Virtual(_) => None,
    };
    name.map(compact_item_name)
        .unwrap_or_else(|| "?".to_string())
}

/// Label for a slot that may not have a signal chosen yet.
pub(crate) fn optional_signal_label(
    catalog: &PrototypeCatalog,
    signal: Option<SignalId>,
) -> String {
    signal
        .map(|signal| signal_short_label(catalog, signal))
        .unwrap_or_else(|| "--".to_string())
}
