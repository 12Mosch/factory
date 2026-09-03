use super::common::test_app;
use bevy::prelude::*;
use bevy::time::{Fixed, TimeUpdateStrategy, Virtual};
use factory_app::constants::MAX_SIM_CATCH_UP_TICKS;
use factory_app::resources::{FixedStepCatchUpStats, SimResource};
use std::time::Duration;

#[test]
fn render_gaps_have_bounded_catch_up_and_report_discarded_backlog() {
    for gap_ms in [100, 250, 500] {
        let gap = Duration::from_millis(gap_ms);
        let mut app = test_app(Duration::ZERO);
        // Complete startup without advancing either clock so the following
        // update models a gap between two established render frames.
        app.update();
        app.world_mut()
            .insert_resource(TimeUpdateStrategy::ManualDuration(gap));
        let before_tick = app.world().resource::<SimResource>().read().tick_count();

        app.update();

        let after_tick = app.world().resource::<SimResource>().read().tick_count();
        assert_eq!(
            after_tick - before_tick,
            u64::from(MAX_SIM_CATCH_UP_TICKS),
            "a {gap_ms} ms frame gap must not run more than the catch-up ceiling"
        );

        let max_delta = app.world().resource::<Time<Virtual>>().max_delta();
        let timestep = app.world().resource::<Time<Fixed>>().timestep();
        assert_eq!(max_delta, timestep * MAX_SIM_CATCH_UP_TICKS);

        let stats = app.world().resource::<FixedStepCatchUpStats>();
        let expected_dropped = gap - max_delta;
        assert_eq!(stats.fixed_ticks_this_frame, MAX_SIM_CATCH_UP_TICKS);
        assert_eq!(stats.peak_fixed_ticks_per_frame, MAX_SIM_CATCH_UP_TICKS);
        assert_eq!(stats.capped_frames, 1);
        assert_eq!(stats.dropped_time_this_frame, expected_dropped);
        assert_eq!(stats.total_dropped_time, expected_dropped);

        app.world_mut()
            .insert_resource(TimeUpdateStrategy::ManualDuration(timestep));
        app.update();

        let recovered_tick = app.world().resource::<SimResource>().read().tick_count();
        assert_eq!(
            recovered_tick - after_tick,
            1,
            "discarded wall time must not remain as a fixed-step backlog"
        );
        let stats = app.world().resource::<FixedStepCatchUpStats>();
        assert_eq!(stats.fixed_ticks_this_frame, 1);
        assert_eq!(stats.capped_frames, 1);
        assert_eq!(stats.dropped_time_this_frame, Duration::ZERO);
        assert_eq!(stats.total_dropped_time, expected_dropped);
    }
}
