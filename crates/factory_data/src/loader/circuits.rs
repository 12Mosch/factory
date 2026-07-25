use crate::error::PrototypeLoadError;
use crate::ids::VirtualSignalId;
use crate::model::{EntityPrototype, VirtualSignalKind, VirtualSignalPrototype};
use crate::raw::RawVirtualSignalPrototype;

pub(super) fn load_virtual_signals(
    signals: Vec<RawVirtualSignalPrototype>,
) -> Result<Vec<VirtualSignalPrototype>, PrototypeLoadError> {
    signals
        .into_iter()
        .map(|signal| {
            Ok(VirtualSignalPrototype {
                id: VirtualSignalId::new(signal.id),
                name: signal.name,
                kind: signal.kind,
            })
        })
        .collect()
}

/// Cross-checks circuit content that only makes sense as a whole: wildcard
/// signals must be unique, and arithmetic or decider combinators need both a
/// connector and the wildcard vocabulary their operand editors rely on.
pub(super) fn validate_circuit_content(
    entities: &[EntityPrototype],
    virtual_signals: &[VirtualSignalPrototype],
) -> Result<(), PrototypeLoadError> {
    for kind in [
        VirtualSignalKind::Each,
        VirtualSignalKind::Anything,
        VirtualSignalKind::Everything,
    ] {
        if virtual_signals
            .iter()
            .filter(|signal| signal.kind == kind)
            .count()
            > 1
        {
            return Err(PrototypeLoadError::InvalidCircuitMetadata {
                entity: format!("{kind:?}"),
                detail: "each wildcard signal kind may be declared at most once",
            });
        }
    }

    let has_operand_combinator = entities.iter().any(|prototype| {
        prototype.combinator.is_some_and(|combinator| {
            matches!(
                combinator.kind,
                crate::model::CombinatorKind::Arithmetic | crate::model::CombinatorKind::Decider
            )
        })
    });
    if has_operand_combinator {
        for (kind, detail) in [
            (
                VirtualSignalKind::Each,
                "catalogs with arithmetic or decider combinators must declare an Each wildcard signal",
            ),
            (
                VirtualSignalKind::Anything,
                "catalogs with arithmetic or decider combinators must declare an Anything wildcard signal",
            ),
            (
                VirtualSignalKind::Everything,
                "catalogs with arithmetic or decider combinators must declare an Everything wildcard signal",
            ),
        ] {
            if !virtual_signals.iter().any(|signal| signal.kind == kind) {
                return Err(PrototypeLoadError::InvalidCircuitMetadata {
                    entity: "combinator".to_string(),
                    detail,
                });
            }
        }
    }

    Ok(())
}
