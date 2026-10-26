use bevy_ecs::world::World;

use crate::error::JournalRefusal;
use crate::hosting::Hosting;
use crate::messages::journal_command::JournalCommand;
use crate::server::checkpoints::{Checkpoints, JournalTaken};
use crate::server::reports::Reports;

/// The placed players' commands of the journal, which only a local server takes: each goes to
/// what keeps it, a save or a load to the checkpoints, a report to the reports.
#[derive(Debug)]
pub struct JournalCommands;

impl JournalCommands {
    /// Takes `command` of a placed player on a local server: a save, a load or a report, as
    /// `Checkpoints::save`, `Checkpoints::load_latest` and `Reports::take` take them; why not,
    /// when it refuses it.
    pub fn take(
        world: &mut World,
        command: JournalCommand,
    ) -> Result<JournalTaken, JournalRefusal> {
        if *world.resource::<Hosting>() != Hosting::Local {
            return Err(JournalRefusal::NotLocal);
        }
        match command {
            JournalCommand::Save => Checkpoints::save(world),
            JournalCommand::LoadLatest => Checkpoints::load_latest(world),
            JournalCommand::Report(key) => {
                Reports::take(world, key)?;
                Ok(JournalTaken::Planned)
            }
        }
    }
}
