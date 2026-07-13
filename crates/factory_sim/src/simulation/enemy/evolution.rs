use super::*;

impl Simulation {
    pub(super) fn advance_evolution_time(&mut self) {
        let Some(cfg) = self.gameplay().copied() else {
            return;
        };
        if self
            .tick
            .is_multiple_of(u64::from(cfg.evolution_time_interval_ticks))
        {
            self.add_evolution_points(u32::from(cfg.evolution_time_points));
        }
    }

    pub(super) fn add_pollution_evolution(&mut self, absorbed_micro: u64) {
        let Some(cfg) = self.gameplay().copied() else {
            return;
        };
        let per_point = u64::from(cfg.evolution_pollution_units_per_point) * 1_000_000;
        self.enemies.pollution_evolution_micro_remainder = self
            .enemies
            .pollution_evolution_micro_remainder
            .saturating_add(absorbed_micro);
        let points = self.enemies.pollution_evolution_micro_remainder / per_point;
        self.enemies.pollution_evolution_micro_remainder %= per_point;
        self.add_evolution_points(points.min(u64::from(u32::MAX)) as u32);
    }

    pub(super) fn add_evolution_points(&mut self, raw: u32) {
        let scaled = raw
            .saturating_mul(u32::from(self.config.runtime.evolution_rate_percent))
            .saturating_add(self.enemies.evolution_remainder);
        self.enemies.evolution_remainder = scaled % 100;
        self.enemies.evolution_points =
            u16::try_from((u32::from(self.enemies.evolution_points) + scaled / 100).min(10_000))
                .unwrap_or(10_000);
    }
}
