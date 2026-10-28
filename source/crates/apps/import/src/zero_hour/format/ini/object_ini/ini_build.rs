use campfire_math::Num;

use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::IniLine;

/// An airfield's `ParkingPlaceBehavior`: its `NumRows` and `NumCols` of places, whether it
/// `ParkInHangars`, its `HealAmountPerSecond` and its `ApproachHeight`, each 0 or no by default, as
/// the module's data starts.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct IniParking {
    pub(crate) rows: u8,
    pub(crate) cols: u8,
    pub(crate) hangars: bool,
    pub(crate) heal: f32,
    pub(crate) approach_height: f32,
}

/// A `ProductionUpdate`'s doors, as `IniBuild` reads them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct IniDoors {
    pub(crate) count: u8,
    pub(crate) opening_ms: u32,
    pub(crate) open_ms: u32,
    pub(crate) closing_ms: u32,
}

/// What an object gives production, as `ThingTemplate` and its modules read it: its `BuildCost`,
/// its `RefundValue`, 0 when it gives none, as `BuildAssistant::update` reads it, its `BuildTime`
/// in seconds, its `CommandSet`, its `Prerequisites`, its `ProductionUpdate`'s `MaxQueueEntries`
/// when it has the module, and its doors, the points of its exit module when it has one that names
/// them, its `ParkingPlaceBehavior`, an airfield's, which makes a helicopter at its model's helipad
/// bone and parks its jets, and its `PlacementViewAngle`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IniBuild {
    pub(crate) cost: u16,
    pub(crate) refund: u16,
    pub(crate) time: f32,
    pub(crate) command_set: Option<String>,
    pub(crate) prerequisites: Vec<Prerequisite>,
    pub(crate) queue: Option<i32>,
    pub(crate) exit: Option<ProductionExit>,
    pub(crate) parking: Option<IniParking>,
    /// Its `ProductionUpdate`'s doors: its `NumDoorAnimations`, and its `DoorOpeningTime`,
    /// `DoorWaitOpenTime` and `DoorCloseTime` in milliseconds, each 0 when absent.
    pub(crate) doors: IniDoors,
    /// Its `PlacementViewAngle` in degrees, the angle the cursor's building shows at as a player
    /// places it, none when it gives none; a value that is not a number is refused.
    pub(crate) placement_angle: Option<Num>,
    /// Its `BuildVariations`, the objects made in its place, one drawn for each, its last line's.
    pub(crate) variations: Vec<String>,
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
/// which the import does not read. A `SupplyCenterProductionExitUpdate` also starts each
/// gatherer that leaves it gathering (`setForceWantingState`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct ProductionExit {
    pub(crate) create: [f32; 2],
    pub(crate) rally: [f32; 2],
    pub(crate) starts_gatherers: bool,
}

/// A behavior module whose fields production reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProductionModule {
    Queue,
    Exit,
    Parking,
}

impl IniBuild {
    /// `ProductionUpdate`'s `MaxQueueEntries` when the module gives none.
    const QUEUE: i32 = 9;

    /// Takes an object's field `word` the build reads; whether it was one.
    pub(crate) fn field(&mut self, word: &str, at: &IniLine<'_>) -> Result<bool, IniError> {
        match word {
            "buildcost" => self.cost = Self::unsigned_short(at)?,
            "refundvalue" => self.refund = Self::unsigned_short(at)?,
            "buildtime" => self.time = at.real()?,
            "commandset" => self.command_set = Some(at.value()?.to_owned()),
            // `parseAsciiStringVector` takes each name of the line, in place of the last line's.
            "buildvariations" => {
                at.value()?;
                self.variations = at.tokens[1..].iter().map(|&name| name.to_owned()).collect();
            }
            "placementviewangle" => {
                let angle = game_float::to_num(at.real()?)
                    .map_err(|_| IniError::Range { line: at.number })?;
                self.placement_angle = Some(angle);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// The value of a field of an unsigned short, as `INI::parseUnsignedShort` reads it.
    fn unsigned_short(at: &IniLine<'_>) -> Result<u16, IniError> {
        u16::try_from(at.int()?).map_err(|_| IniError::Range { line: at.number })
    }

    /// Opens the behavior module `module`, of the name its line gives in lowercase: the one of
    /// those production reads, which starts as the game starts it.
    pub(crate) fn module(&mut self, module: &str) -> Option<ProductionModule> {
        match module {
            "productionupdate" => {
                self.queue = Some(Self::QUEUE);
                Some(ProductionModule::Queue)
            }
            "defaultproductionexitupdate"
            | "queueproductionexitupdate"
            | "supplycenterproductionexitupdate" => {
                self.exit = Some(ProductionExit {
                    starts_gatherers: module == "supplycenterproductionexitupdate",
                    ..ProductionExit::default()
                });
                Some(ProductionModule::Exit)
            }
            "parkingplacebehavior" => {
                self.parking = Some(IniParking::default());
                Some(ProductionModule::Parking)
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
        let count = || {
            let count = at.int()?;
            u8::try_from(count).map_err(|_| IniError::Range { line: at.number })
        };
        let ms = || {
            let ms = at.int()?;
            u32::try_from(ms).map_err(|_| IniError::Range { line: at.number })
        };
        match (module, word) {
            (ProductionModule::Queue, "maxqueueentries") => self.queue = Some(at.int()?),
            (ProductionModule::Queue, "numdooranimations") => self.doors.count = count()?,
            (ProductionModule::Queue, "dooropeningtime") => self.doors.opening_ms = ms()?,
            (ProductionModule::Queue, "doorwaitopentime") => self.doors.open_ms = ms()?,
            (ProductionModule::Queue, "doorclosetime") => self.doors.closing_ms = ms()?,
            (ProductionModule::Exit, "unitcreatepoint") => {
                self.exit.get_or_insert_default().create = point()?;
            }
            (ProductionModule::Exit, "naturalrallypoint") => {
                self.exit.get_or_insert_default().rally = point()?;
            }
            (ProductionModule::Parking, "numrows") => {
                self.parking.get_or_insert_default().rows = count()?;
            }
            (ProductionModule::Parking, "numcols") => {
                self.parking.get_or_insert_default().cols = count()?;
            }
            (ProductionModule::Parking, "parkinhangars") => {
                self.parking.get_or_insert_default().hangars = at.yes()?;
            }
            (ProductionModule::Parking, "healamountpersecond") => {
                self.parking.get_or_insert_default().heal = at.real()?;
            }
            (ProductionModule::Parking, "approachheight") => {
                self.parking.get_or_insert_default().approach_height = at.real()?;
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

impl Default for IniBuild {
    /// As `ThingTemplate` starts: free, of no refund, a second to build, of no command set,
    /// prerequisite, production module, door or variation.
    fn default() -> Self {
        Self {
            cost: 0,
            refund: 0,
            time: 1.0,
            command_set: None,
            prerequisites: Vec::new(),
            queue: None,
            exit: None,
            parking: None,
            doors: IniDoors::default(),
            placement_angle: None,
            variations: Vec::new(),
        }
    }
}
