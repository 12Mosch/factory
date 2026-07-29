//! The motion model: forces in, velocity out, in integers throughout.
//!
//! One tile is one metre and one tick is a sixtieth of a second. That fixes the
//! whole conversion: an acceleration of `F/m` metres per second squared is
//! `F/m * POSITION_SCALE / 3600` fixed-point units per tick per tick, scaled up
//! by [`TRAIN_VELOCITY_SCALE`] so a tick's worth of it is a whole number rather
//! than a rounded-away fraction.
//!
//! Every quantity here is derived from the same three per-prototype numbers —
//! weight, tractive force, braking force — so acceleration, top speed, and
//! stopping distance are three readings of one model rather than three
//! approximations that can disagree. [`braking_distance_fixed`] exists for that
//! reason: a station that wants to stop a train on a mark has to ask this
//! model, not build a second one.

use crate::rolling_stock::{
    ROLLING_RESISTANCE_NEWTONS_PER_TONNE, TRAIN_VELOCITY_SCALE, TrainForces, TrainThrottle,
};
use factory_data::POSITION_SCALE;

/// Ticks per second squared: the divisor that turns metres per second squared
/// into fixed-point units per tick per tick.
const TICKS_PER_SECOND_SQUARED: i128 = 3_600;

/// Change in velocity one tick of `force_newtons` on `weight_kilograms`
/// produces, in [`TRAIN_VELOCITY_SCALE`] units.
///
/// Evaluated as a single 128-bit expression so nothing rounds until the final
/// divide: rounding an intermediate would make the same train accelerate
/// differently depending on how the forces happened to be grouped. The one
/// truncation left is deterministic, and it is toward zero, so a force too
/// small to move a train never accelerates it by a phantom unit.
pub(in crate::simulation) fn velocity_delta(force_newtons: i64, weight_kilograms: i64) -> i64 {
    if weight_kilograms <= 0 {
        return 0;
    }
    let numerator =
        i128::from(force_newtons) * i128::from(POSITION_SCALE) * i128::from(TRAIN_VELOCITY_SCALE);
    let denominator = i128::from(weight_kilograms) * TICKS_PER_SECOND_SQUARED;
    (numerator / denominator) as i64
}

/// Resistance a train of `weight_kilograms` drags against, in newtons.
pub(in crate::simulation) fn resistance_newtons(weight_kilograms: i64) -> i64 {
    weight_kilograms * ROLLING_RESISTANCE_NEWTONS_PER_TONNE / 1_000
}

/// The train's velocity after one tick under `throttle`.
///
/// Resistance and braking always oppose motion, and neither may push a train
/// backwards: a train slowed to a stop stops rather than reversing under its
/// own brakes. Tractive force is the only thing that can change the sign of the
/// velocity, which is what makes `Reverse` a command to drive backwards rather
/// than a second way to brake.
pub(in crate::simulation) fn stepped_velocity(
    velocity: i64,
    throttle: TrainThrottle,
    forces: TrainForces,
) -> i64 {
    let weight = forces.weight_kilograms;
    if weight <= 0 {
        return 0;
    }

    let drive = throttle.drive_sign() * forces.tractive_force_newtons;
    let opposing = resistance_newtons(weight)
        + match throttle {
            TrainThrottle::Brake => forces.braking_force_newtons,
            _ => 0,
        };

    let driven = velocity + velocity_delta(drive, weight);
    // Opposition is applied after the drive and is clamped at a standstill, so
    // it can only ever remove speed.
    let opposition = velocity_delta(opposing, weight);
    let slowed = if driven > 0 {
        (driven - opposition).max(0)
    } else if driven < 0 {
        (driven + opposition).min(0)
    } else {
        0
    };

    slowed.clamp(-forces.max_speed, forces.max_speed)
}

/// Distance a train needs to come to a stop from `velocity` under full braking,
/// in fixed-point units.
///
/// This is the exact sum the simulation would produce, not a continuous
/// approximation of it: velocity is decremented once per tick and then spent,
/// so the distance covered is the arithmetic series of the velocities that
/// remain positive. A station asking "may I still stop at the mark?" gets the
/// same answer the tick loop will produce.
pub fn braking_distance_fixed(velocity: i64, forces: TrainForces) -> i64 {
    let weight = forces.weight_kilograms;
    if weight <= 0 {
        return 0;
    }
    let speed = velocity.unsigned_abs() as i128;
    if speed == 0 {
        return 0;
    }
    let deceleration = i128::from(velocity_delta(
        forces.braking_force_newtons + resistance_newtons(weight),
        weight,
    ));
    if deceleration <= 0 {
        return i64::MAX;
    }

    // Ticks spent above a standstill, and the series of the velocities spent in
    // them: `n * v - decel * n * (n + 1) / 2`.
    let ticks = speed / deceleration;
    let travelled = ticks * speed - deceleration * ticks * (ticks + 1) / 2;
    (travelled / i128::from(TRAIN_VELOCITY_SCALE)) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A locomotive on its own: 12 kN on 2 tonnes is 6 m/s^2, which at 1024
    /// units per tile and 60 ticks per second is 1_706_666 velocity units per
    /// tick per tick.
    #[test]
    fn acceleration_follows_force_over_mass() {
        assert_eq!(velocity_delta(12_000, 2_000), 1_706_666);
        // Doubling the mass halves the acceleration, and doubling the force
        // restores it: the model is force over mass and nothing else.
        assert_eq!(velocity_delta(12_000, 4_000), 853_333);
        assert_eq!(velocity_delta(24_000, 4_000), 1_706_666);
    }

    #[test]
    fn a_zero_weight_train_never_accelerates() {
        assert_eq!(velocity_delta(12_000, 0), 0);
        assert_eq!(velocity_delta(12_000, -5), 0);
    }

    fn locomotive_forces() -> TrainForces {
        TrainForces {
            weight_kilograms: 2_000,
            tractive_force_newtons: 12_000,
            braking_force_newtons: 24_000,
            max_speed: 1_229 * TRAIN_VELOCITY_SCALE,
        }
    }

    #[test]
    fn an_open_throttle_accelerates_up_to_the_top_speed() {
        let forces = locomotive_forces();
        let mut velocity = 0;
        for _ in 0..10_000 {
            velocity = stepped_velocity(velocity, TrainThrottle::Forward, forces);
        }

        assert_eq!(velocity, forces.max_speed);
    }

    /// Resistance is the only force on a coasting train, so it slows down and
    /// then stays stopped rather than rolling backwards.
    #[test]
    fn coasting_slows_to_a_stop_and_stays_there() {
        let forces = locomotive_forces();
        let mut velocity = 100 * TRAIN_VELOCITY_SCALE;
        for _ in 0..100_000 {
            velocity = stepped_velocity(velocity, TrainThrottle::Coast, forces);
            assert!(velocity >= 0, "resistance must never reverse a train");
        }

        assert_eq!(velocity, 0);
    }

    #[test]
    fn braking_never_pushes_a_train_the_other_way() {
        let forces = locomotive_forces();
        let mut velocity = -50 * TRAIN_VELOCITY_SCALE;
        for _ in 0..100_000 {
            velocity = stepped_velocity(velocity, TrainThrottle::Brake, forces);
            assert!(velocity <= 0);
        }

        assert_eq!(velocity, 0);
    }

    /// The whole reason the stopping distance is stated as a function rather
    /// than measured: it has to agree with what the tick loop actually does,
    /// because a station will trust it before the train has moved.
    #[test]
    fn the_predicted_braking_distance_is_the_one_the_model_produces() {
        let forces = locomotive_forces();
        for speed_tiles_per_tick in [1, 10, 100, 1_229] {
            let start = speed_tiles_per_tick * TRAIN_VELOCITY_SCALE;
            let predicted = braking_distance_fixed(start, forces);

            let mut velocity = start;
            let mut travelled = 0_i128;
            while velocity > 0 {
                velocity = stepped_velocity(velocity, TrainThrottle::Brake, forces);
                travelled += i128::from(velocity);
            }
            let simulated = (travelled / i128::from(TRAIN_VELOCITY_SCALE)) as i64;

            assert_eq!(
                predicted, simulated,
                "braking from {speed_tiles_per_tick} units per tick"
            );
        }
    }

    /// A train with no brakes still stops, because resistance is part of the
    /// same sum — which is what stops the stopping distance from being
    /// infinite for a runaway wagon that lost its locomotive.
    #[test]
    fn a_brakeless_train_still_stops_on_resistance_alone() {
        let forces = TrainForces {
            weight_kilograms: 2_000,
            tractive_force_newtons: 0,
            braking_force_newtons: 0,
            max_speed: TRAIN_VELOCITY_SCALE,
        };

        assert!(braking_distance_fixed(TRAIN_VELOCITY_SCALE, forces) > 0);
        assert_eq!(braking_distance_fixed(0, forces), 0);
        assert_eq!(
            braking_distance_fixed(TRAIN_VELOCITY_SCALE, TrainForces::default()),
            0
        );
    }
}
