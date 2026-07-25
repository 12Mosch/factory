//! Circuit-network state: wire connections, signal values, per-entity
//! conditions, and combinator configuration.
//!
//! This module holds only data and pure helpers. Network connectivity and
//! per-tick evaluation live in `simulation::circuit_ops`, mirroring how
//! `fluids` and `simulation::fluid_ops` are split.
//!
//! # Determinism
//!
//! Signal values are accumulated with wrapping `i32` arithmetic. Wrapping
//! addition is associative and commutative, so the merged value of a network
//! does not depend on the order sources are visited. Saturating arithmetic
//! would not have that property and would make the result depend on iteration
//! order once a network overflows.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::ids::EntityId;
use factory_data::{FluidId, ItemId, VirtualSignalId};

/// A value channel on a circuit network.
///
/// The derived ordering is the canonical signal order used everywhere a signal
/// set is iterated, so combinator output and presentation stay stable.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum SignalId {
    Item(ItemId),
    Fluid(FluidId),
    Virtual(VirtualSignalId),
}

/// The two independent wire colors. Red and green form separate networks; an
/// entity wired with both participates in both and reads their merged values.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum WireColor {
    Red,
    Green,
}

impl WireColor {
    pub const ALL: [Self; 2] = [Self::Red, Self::Green];

    pub const fn index(self) -> usize {
        self as usize
    }
}

/// Which connector of an entity a wire attaches to. Entities declaring
/// [`factory_data::CircuitPortLayout::Single`] only ever use [`Self::Single`];
/// combinators use [`Self::Input`] and [`Self::Output`].
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ConnectorPort {
    Single,
    Input,
    Output,
}

impl ConnectorPort {
    pub const ALL: [Self; 3] = [Self::Single, Self::Input, Self::Output];

    /// Whether `layout` permits wires on this port.
    pub fn is_valid_for(self, layout: factory_data::CircuitPortLayout) -> bool {
        match layout {
            factory_data::CircuitPortLayout::Single => self == Self::Single,
            factory_data::CircuitPortLayout::InputOutput => {
                matches!(self, Self::Input | Self::Output)
            }
        }
    }
}

/// One end of a wire: an entity plus the connector the wire lands on.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct CircuitNode {
    pub entity_id: EntityId,
    pub port: ConnectorPort,
}

impl CircuitNode {
    pub const fn new(entity_id: EntityId, port: ConnectorPort) -> Self {
        Self { entity_id, port }
    }
}

/// Wires attached to one entity, stored symmetrically: both endpoints of a
/// wire record each other. Symmetry keeps removal local (dropping an entity
/// only has to visit the neighbors it lists) and lets network building walk
/// the graph without an inverse index.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct CircuitConnections {
    /// Neighbors keyed by `(local port, color)`, each list sorted and free of
    /// duplicates so the serialized form and the rebuild order are canonical.
    links: BTreeMap<(ConnectorPort, WireColor), Vec<CircuitNode>>,
}

impl CircuitConnections {
    pub fn is_empty(&self) -> bool {
        self.links.values().all(|neighbors| neighbors.is_empty())
    }

    pub fn neighbors(&self, port: ConnectorPort, color: WireColor) -> &[CircuitNode] {
        self.links
            .get(&(port, color))
            .map_or(&[], |neighbors| neighbors.as_slice())
    }

    /// Every wire attached to this entity, as `(local port, color, neighbor)`.
    pub fn iter(&self) -> impl Iterator<Item = (ConnectorPort, WireColor, CircuitNode)> + '_ {
        self.links.iter().flat_map(|(&(port, color), neighbors)| {
            neighbors
                .iter()
                .map(move |neighbor| (port, color, *neighbor))
        })
    }

    /// Records a neighbor, returning whether the link was new.
    pub fn insert(&mut self, port: ConnectorPort, color: WireColor, neighbor: CircuitNode) -> bool {
        let neighbors = self.links.entry((port, color)).or_default();
        match neighbors.binary_search(&neighbor) {
            Ok(_) => false,
            Err(index) => {
                neighbors.insert(index, neighbor);
                true
            }
        }
    }

    /// Drops a neighbor, returning whether a link was removed.
    pub fn remove(&mut self, port: ConnectorPort, color: WireColor, neighbor: CircuitNode) -> bool {
        let Some(neighbors) = self.links.get_mut(&(port, color)) else {
            return false;
        };
        let Ok(index) = neighbors.binary_search(&neighbor) else {
            return false;
        };
        neighbors.remove(index);
        if neighbors.is_empty() {
            self.links.remove(&(port, color));
        }
        true
    }

    /// Drops every link to `entity_id`, returning the removed wires so the
    /// caller can refund wire items.
    pub fn remove_entity(
        &mut self,
        entity_id: EntityId,
    ) -> Vec<(ConnectorPort, WireColor, CircuitNode)> {
        let removed = self
            .iter()
            .filter(|(_, _, neighbor)| neighbor.entity_id == entity_id)
            .collect::<Vec<_>>();
        for &(port, color, neighbor) in &removed {
            self.remove(port, color, neighbor);
        }
        removed
    }
}

/// How the two operands of a condition compare.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum Comparator {
    Greater,
    Less,
    Equal,
    GreaterOrEqual,
    LessOrEqual,
    NotEqual,
}

impl Comparator {
    pub const ALL: [Self; 6] = [
        Self::Greater,
        Self::Less,
        Self::Equal,
        Self::GreaterOrEqual,
        Self::LessOrEqual,
        Self::NotEqual,
    ];

    pub const fn apply(self, left: i32, right: i32) -> bool {
        match self {
            Self::Greater => left > right,
            Self::Less => left < right,
            Self::Equal => left == right,
            Self::GreaterOrEqual => left >= right,
            Self::LessOrEqual => left <= right,
            Self::NotEqual => left != right,
        }
    }

    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Greater => ">",
            Self::Less => "<",
            Self::Equal => "=",
            Self::GreaterOrEqual => ">=",
            Self::LessOrEqual => "<=",
            Self::NotEqual => "!=",
        }
    }
}

/// The right-hand side of a condition or arithmetic operation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum SignalOperand {
    Signal(SignalId),
    Constant(i32),
}

impl Default for SignalOperand {
    fn default() -> Self {
        Self::Constant(0)
    }
}

/// An enable/disable rule evaluated against the merged signals reaching an
/// entity. A `None` condition on a wired entity means "always enabled".
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct CircuitCondition {
    pub left: SignalId,
    pub comparator: Comparator,
    pub right: SignalOperand,
}

/// Per-entity circuit participation. Created lazily: an entity gains this
/// state the first time a wire is attached or a condition is configured, so
/// unwired belts and inserters cost nothing.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct CircuitEntityState {
    pub connections: CircuitConnections,
    /// Gates the entity's own work. Only meaningful on prototypes whose
    /// connector is `controllable`.
    pub enable_condition: Option<CircuitCondition>,
    /// Publishes the entity's contents onto its networks. Only meaningful on
    /// prototypes whose connector declares `reads_contents`.
    pub read_contents: bool,
    /// Channel an accumulator reports its charge percentage on. Ignored by
    /// every other kind.
    pub charge_output_signal: Option<SignalId>,
}

impl CircuitEntityState {
    /// Whether the state still carries information worth keeping. Used to drop
    /// the map entry again once the player removes the last wire and resets
    /// the configuration.
    pub fn is_inert(&self) -> bool {
        self.connections.is_empty()
            && self.enable_condition.is_none()
            && !self.read_contents
            && self.charge_output_signal.is_none()
    }
}

/// One configured row of a constant combinator.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ConstantSignalSlot {
    pub signal: Option<SignalId>,
    pub value: i32,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ConstantCombinatorState {
    pub enabled: bool,
    pub slots: Vec<ConstantSignalSlot>,
}

impl ConstantCombinatorState {
    pub fn with_slot_count(slot_count: usize) -> Self {
        Self {
            enabled: true,
            slots: vec![ConstantSignalSlot::default(); slot_count],
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum ArithmeticOperation {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Power,
    LeftShift,
    RightShift,
    And,
    Or,
    Xor,
}

impl ArithmeticOperation {
    pub const ALL: [Self; 11] = [
        Self::Add,
        Self::Subtract,
        Self::Multiply,
        Self::Divide,
        Self::Modulo,
        Self::Power,
        Self::LeftShift,
        Self::RightShift,
        Self::And,
        Self::Or,
        Self::Xor,
    ];

    /// Applies the operation with wrapping semantics. Division and modulo by
    /// zero yield zero rather than trapping, matching how the reference game
    /// keeps a misconfigured combinator from stalling the tick.
    pub const fn apply(self, left: i32, right: i32) -> i32 {
        match self {
            Self::Add => left.wrapping_add(right),
            Self::Subtract => left.wrapping_sub(right),
            Self::Multiply => left.wrapping_mul(right),
            Self::Divide => {
                if right == 0 {
                    0
                } else {
                    left.wrapping_div(right)
                }
            }
            Self::Modulo => {
                if right == 0 {
                    0
                } else {
                    left.wrapping_rem(right)
                }
            }
            Self::Power => wrapping_pow_i32(left, right),
            // Shift distances are masked to 0..=31 so a large or negative
            // operand cannot produce an undefined shift.
            Self::LeftShift => left.wrapping_shl(right as u32),
            Self::RightShift => left.wrapping_shr(right as u32),
            Self::And => left & right,
            Self::Or => left | right,
            Self::Xor => left ^ right,
        }
    }

    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "*",
            Self::Divide => "/",
            Self::Modulo => "%",
            Self::Power => "^",
            Self::LeftShift => "<<",
            Self::RightShift => ">>",
            Self::And => "AND",
            Self::Or => "OR",
            Self::Xor => "XOR",
        }
    }
}

/// Exponentiation with a wrapping, non-trapping contract. A negative exponent
/// has no integer result, so it yields zero.
const fn wrapping_pow_i32(base: i32, exponent: i32) -> i32 {
    if exponent < 0 {
        return 0;
    }
    let mut result: i32 = 1;
    let mut remaining = exponent as u32;
    let mut factor = base;
    while remaining > 0 {
        if remaining & 1 == 1 {
            result = result.wrapping_mul(factor);
        }
        factor = factor.wrapping_mul(factor);
        remaining >>= 1;
    }
    result
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ArithmeticCombinatorState {
    pub left: SignalOperand,
    pub operation: ArithmeticOperation,
    pub right: SignalOperand,
    /// Where the result is written. With an `Each` left operand this may also
    /// be `Each`, producing one output per input signal.
    pub output: Option<SignalId>,
    /// Result computed from the previous tick's inputs, published this tick.
    /// Storing the output rather than recomputing it inline is what gives
    /// every combinator a uniform one-tick delay.
    pub outputs: Vec<(SignalId, i32)>,
}

impl Default for ArithmeticCombinatorState {
    fn default() -> Self {
        Self {
            left: SignalOperand::Constant(0),
            operation: ArithmeticOperation::Add,
            right: SignalOperand::Constant(0),
            output: None,
            outputs: Vec::new(),
        }
    }
}

/// What a decider combinator emits for the signals that pass its condition.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum DeciderOutputValue {
    /// Emit `1` for each passing signal.
    #[default]
    One,
    /// Emit the input value of the output signal.
    InputCount,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct DeciderCombinatorState {
    /// `None` until the player picks a signal; an unconfigured decider never
    /// passes and so emits nothing.
    pub left: Option<SignalId>,
    pub comparator: Comparator,
    pub right: SignalOperand,
    pub output: Option<SignalId>,
    pub output_value: DeciderOutputValue,
    /// Result computed from the previous tick's inputs, published this tick.
    pub outputs: Vec<(SignalId, i32)>,
}

impl Default for DeciderCombinatorState {
    fn default() -> Self {
        Self {
            left: None,
            comparator: Comparator::Greater,
            right: SignalOperand::Constant(0),
            output: None,
            output_value: DeciderOutputValue::One,
            outputs: Vec::new(),
        }
    }
}

/// Lit state of a lamp, derived each tick from its enable condition. Durable
/// so a loaded save renders correctly before the first tick runs.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct LampState {
    pub lit: bool,
}

/// The merged signal values on one network, kept sorted by [`SignalId`].
///
/// Zero-valued signals are never stored: a signal that nets out to zero is
/// indistinguishable from an absent one, and dropping it keeps the canonical
/// form unique.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SignalSet {
    signals: Vec<(SignalId, i32)>,
}

impl SignalSet {
    pub fn clear(&mut self) {
        self.signals.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.signals.is_empty()
    }

    pub fn len(&self) -> usize {
        self.signals.len()
    }

    pub fn value(&self, signal: SignalId) -> i32 {
        self.signals
            .binary_search_by_key(&signal, |(id, _)| *id)
            .map_or(0, |index| self.signals[index].1)
    }

    pub fn iter(&self) -> impl Iterator<Item = (SignalId, i32)> + '_ {
        self.signals.iter().copied()
    }

    pub fn as_slice(&self) -> &[(SignalId, i32)] {
        &self.signals
    }

    /// Merges `amount` into `signal` with wrapping addition, so the result is
    /// independent of the order contributions arrive in.
    pub fn add(&mut self, signal: SignalId, amount: i32) {
        if amount == 0 {
            return;
        }
        match self.signals.binary_search_by_key(&signal, |(id, _)| *id) {
            Ok(index) => {
                let total = self.signals[index].1.wrapping_add(amount);
                if total == 0 {
                    self.signals.remove(index);
                } else {
                    self.signals[index].1 = total;
                }
            }
            Err(index) => self.signals.insert(index, (signal, amount)),
        }
    }

    pub fn extend_from(&mut self, other: &Self) {
        for (signal, value) in other.iter() {
            self.add(signal, value);
        }
    }
}

/// Clamps a `u64` count into the signal value range. Networks carry `i32`
/// values, so an inventory larger than `i32::MAX` reports the maximum rather
/// than wrapping into a negative count.
pub fn signal_value_from_count(count: u64) -> i32 {
    i32::try_from(count).unwrap_or(i32::MAX)
}
