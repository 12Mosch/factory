use super::*;

pub(super) fn apply(
    sim: &mut Simulation,
    command: &SimCommand,
) -> Result<SimCommandEffect, SimCommandError> {
    match command {
        SimCommand::BuildRedScienceResearchFixture => sim.build_red_science_research_fixture(),
        SimCommand::BuildChemicalScienceFactoryFixture => {
            sim.build_chemical_science_factory_fixture();
        }
        SimCommand::RunChemicalScienceFactoryProgram => {
            sim.run_chemical_science_factory_program();
        }
        _ => unreachable!("non-fixture command routed to fixture dispatcher"),
    }
    Ok(SimCommandEffect::None)
}
