use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::IniLine;

/// What an object gives supply, as its modules read it: the boxes of its
/// `SupplyWarehouseDockUpdate`, whether it has a `SupplyCenterDockUpdate`, which takes a
/// gatherer's boxes, the numbers of its gatherer's AI module, and its `SpawnBehavior`s.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct IniSupply {
    pub(crate) dock: Option<SupplyDock>,
    pub(crate) center: bool,
    pub(crate) gatherer: Option<SupplyGatherer>,
    pub(crate) spawns: Vec<IniSpawn>,
}

/// A `SupplyWarehouseDockUpdate`: its `StartingBoxes`, and whether its object goes as its last
/// box goes, its `DeleteWhenEmpty`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SupplyDock {
    pub(crate) boxes: i32,
    pub(crate) delete_when_empty: bool,
}

/// A gatherer's AI module, `SupplyTruckAIUpdate`, `WorkerAIUpdate` or `ChinookAIUpdate`: its
/// `MaxBoxes`, its `SupplyCenterActionDelay` and `SupplyWarehouseActionDelay` in milliseconds, and
/// its `SupplyWarehouseScanDistance`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SupplyGatherer {
    pub(crate) max_boxes: i32,
    pub(crate) center_delay_ms: i32,
    pub(crate) warehouse_delay_ms: i32,
    pub(crate) scan_distance: f32,
}

/// A `SpawnBehavior`: its `SpawnNumber`, whether it spawns once, its `OneShot`, and its
/// `SpawnTemplateName`s, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct IniSpawn {
    pub(crate) number: i32,
    pub(crate) one_shot: bool,
    pub(crate) templates: Vec<String>,
}

/// A behavior module whose fields supply reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SupplyModule {
    Dock,
    Gatherer,
    Spawn,
}

impl IniSupply {
    /// Opens the behavior module `module`, of the name its line gives in lowercase: the one of
    /// those supply reads, which starts as the game starts it; none for another, or for a
    /// `SupplyCenterDockUpdate`, whose fields supply does not read.
    pub(crate) fn module(&mut self, module: &str) -> Option<SupplyModule> {
        match module {
            "supplywarehousedockupdate" => {
                self.dock = Some(SupplyDock {
                    boxes: 1,
                    delete_when_empty: false,
                });
                Some(SupplyModule::Dock)
            }
            "supplycenterdockupdate" => {
                self.center = true;
                None
            }
            "supplytruckaiupdate" | "workeraiupdate" | "chinookaiupdate" => {
                self.gatherer = Some(SupplyGatherer {
                    max_boxes: 0,
                    center_delay_ms: 0,
                    warehouse_delay_ms: 0,
                    scan_distance: 100.0,
                });
                Some(SupplyModule::Gatherer)
            }
            "spawnbehavior" => {
                self.spawns.push(IniSpawn::default());
                Some(SupplyModule::Spawn)
            }
            _ => None,
        }
    }

    /// A field of the open module `module`.
    pub(crate) fn module_field(
        &mut self,
        module: SupplyModule,
        word: &str,
        at: &IniLine<'_>,
    ) -> Result<(), IniError> {
        match module {
            SupplyModule::Dock => {
                let dock = self.dock.as_mut().expect("a dock's module is open");
                match word {
                    "startingboxes" => dock.boxes = at.int()?,
                    "deletewhenempty" => dock.delete_when_empty = at.yes()?,
                    _ => {}
                }
            }
            SupplyModule::Gatherer => {
                let gatherer = self.gatherer.as_mut().expect("a gatherer's module is open");
                match word {
                    "maxboxes" => gatherer.max_boxes = at.int()?,
                    "supplycenteractiondelay" => gatherer.center_delay_ms = at.int()?,
                    "supplywarehouseactiondelay" => gatherer.warehouse_delay_ms = at.int()?,
                    "supplywarehousescandistance" => gatherer.scan_distance = at.real()?,
                    _ => {}
                }
            }
            SupplyModule::Spawn => {
                let spawn = self.spawns.last_mut().expect("a spawn's module is open");
                match word {
                    "spawnnumber" => spawn.number = at.int()?,
                    "oneshot" => spawn.one_shot = at.yes()?,
                    // `parseAsciiStringVectorAppend` adds each name of each line.
                    "spawntemplatename" => {
                        at.value()?;
                        let names = at.tokens[1..].iter().map(|&name| name.to_owned());
                        spawn.templates.extend(names);
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// The objects its one-shot spawns make as its object is made, in order: each spawn's
    /// `SpawnNumber`, its templates taken in turn from the first, as `SpawnBehavior::createSpawn`
    /// steps through them.
    pub(crate) fn one_shots(&self) -> impl Iterator<Item = &str> {
        self.spawns
            .iter()
            .filter(|spawn| spawn.one_shot && !spawn.templates.is_empty())
            .flat_map(|spawn| {
                let count = usize::try_from(spawn.number).unwrap_or(0);
                spawn
                    .templates
                    .iter()
                    .cycle()
                    .take(count)
                    .map(String::as_str)
            })
    }
}
