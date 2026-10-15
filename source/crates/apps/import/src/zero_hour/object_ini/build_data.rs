use crate::zero_hour::error::IniError;
use crate::zero_hour::ini_text::IniLine;

/// What an object gives production, as `ThingTemplate` and its modules read it: its `BuildCost`,
/// its `BuildTime` in seconds, its `CommandSet`, its `Prerequisites`, its `ProductionUpdate`'s
/// `MaxQueueEntries` when it has the module, and the points of its exit module when it has one
/// that names them.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BuildData {
    pub(crate) cost: u16,
    pub(crate) time: f32,
    pub(crate) command_set: Option<String>,
    pub(crate) prerequisites: Vec<Prerequisite>,
    pub(crate) queue: Option<i32>,
    pub(crate) exit: Option<ProductionExit>,
}

/// A line of an object's `Prerequisites`: objects any one of which meets it, or a science.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Prerequisite {
    Objects(Vec<String>),
    Science(String),
}

/// An exit module's points, `[x, y]` on the ground in its object's frame, the game's axes: where
/// a unit it makes comes out (`UnitCreatePoint`), and where it goes with no rally point set
/// (`NaturalRallyPoint`). `DefaultProductionExitUpdate`, `QueueProductionExitUpdate` and
/// `SupplyCenterProductionExitUpdate` name both; `SpawnPointProductionExitUpdate` names bones,
/// which the import does not read.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct ProductionExit {
    pub(crate) create: [f32; 2],
    pub(crate) rally: [f32; 2],
}

/// A behavior module whose fields production reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProductionModule {
    Queue,
    Exit,
}

impl BuildData {
    /// `ProductionUpdate`'s `MaxQueueEntries` when the module gives none.
    const QUEUE: i32 = 9;

    /// Takes an object's field `word` the build reads; whether it was one.
    pub(crate) fn field(&mut self, word: &str, at: &IniLine<'_>) -> Result<bool, IniError> {
        match word {
            "buildcost" => {
                let cost = at.int()?;
                self.cost = u16::try_from(cost).map_err(|_| IniError::Range { line: at.number })?;
            }
            "buildtime" => self.time = at.real()?,
            "commandset" => self.command_set = Some(at.value()?.to_owned()),
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Opens the behavior module `module`, of the name its line gives in lowercase: the one of
    /// those production reads, which starts as the game starts it.
    pub(crate) fn module(&mut self, module: &str) -> Option<ProductionModule> {
        match module {
            "productionupdate" => {
                self.queue = Some(BuildData::QUEUE);
                Some(ProductionModule::Queue)
            }
            "defaultproductionexitupdate"
            | "queueproductionexitupdate"
            | "supplycenterproductionexitupdate" => {
                self.exit = Some(ProductionExit::default());
                Some(ProductionModule::Exit)
            }
            _ => None,
        }
    }

    /// A field of the open module `module`.
    pub(crate) fn module_field(
        &mut self,
        module: ProductionModule,
        word: &str,
        at: &IniLine<'_>,
    ) -> Result<(), IniError> {
        let point = || at.coord(["X", "Y", "Z"]).map(|[x, y, _]| [x, y]);
        match (module, word) {
            (ProductionModule::Queue, "maxqueueentries") => self.queue = Some(at.int()?),
            (ProductionModule::Exit, "unitcreatepoint") => {
                self.exit.get_or_insert_default().create = point()?;
            }
            (ProductionModule::Exit, "naturalrallypoint") => {
                self.exit.get_or_insert_default().rally = point()?;
            }
            _ => {}
        }
        Ok(())
    }

    /// A line of its `Prerequisites`, as `parsePrerequisiteUnit` and `parsePrerequisiteScience`
    /// read them: the objects of one line any one of which meets it.
    pub(crate) fn prerequisite(&mut self, word: &str, at: &IniLine<'_>) -> Result<(), IniError> {
        let names = || at.tokens[1..].iter().map(|&name| name.to_owned());
        match word {
            "object" => {
                at.value()?;
                self.prerequisites
                    .push(Prerequisite::Objects(names().collect()));
            }
            "science" => self
                .prerequisites
                .push(Prerequisite::Science(at.value()?.to_owned())),
            _ => {}
        }
        Ok(())
    }
}

impl Default for BuildData {
    /// As `ThingTemplate` starts: free, a second to build, of no command set, prerequisite or
    /// production module.
    fn default() -> BuildData {
        BuildData {
            cost: 0,
            time: 1.0,
            command_set: None,
            prerequisites: Vec::new(),
            queue: None,
            exit: None,
        }
    }
}
