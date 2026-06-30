use super::common::{technology_id_by_name, test_app};
use bevy::prelude::*;
use factory_app::resources::{SimResource, TechnologyWindowState};
use factory_app::ui::technology_panel::TechnologyStartQueueButton;
use std::time::Duration;

#[test]
fn technology_screen_start_button_updates_research_state() {
    let mut app = test_app(Duration::from_secs_f64(1.0 / 60.0));
    let logistics = {
        let sim = &app.world().resource::<SimResource>().sim;
        technology_id_by_name(sim.catalog(), "logistics")
    };
    {
        let mut window = app.world_mut().resource_mut::<TechnologyWindowState>();
        window.open = true;
        window.selected = Some(logistics);
    }
    app.update();

    let mut query = app
        .world_mut()
        .query_filtered::<&mut Interaction, With<TechnologyStartQueueButton>>();
    for mut interaction in query.iter_mut(app.world_mut()) {
        *interaction = Interaction::Pressed;
    }
    app.update();

    assert_eq!(
        app.world().resource::<SimResource>().sim.active_research(),
        Some(logistics)
    );
}
