use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::IniLine;

/// What an object gives supply, as its modules read it: the boxes of its
/// `SupplyWarehouseDockUpdate`, whether it has a `SupplyCenterDockUpdate`, which takes a
/// gatherer's boxes, the numbers of its gatherer's AI module, its `SpawnBehavior`s, and whether it
/// has an `OCLUpdate`, which creates objects over time, as America's supply drop zone sends a
/// cargo plane of crates; its `AutoDepositUpdate`, which pays its player over time; and its
/// `HackInternetAIUpdate`, whose hack pays its player while it stands.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct IniSupply {
    pub(crate) dock: Option<SupplyDock>,
    pub(crate) center: bool,
    pub(crate) gatherer: Option<SupplyGatherer>,
    pub(crate) spawns: Vec<IniSpawn>,
    pub(crate) creates_over_time: bool,
    pub(crate) deposit: Option<AutoDeposit>,
    pub(crate) hacker: Option<InternetHack>,
}

/// A `HackInternetAIUpdate`, as `HackInternetAIUpdateModuleData` reads it: its `UnpackTime`,
/// `PackTime`, `CashUpdateDelay` and `CashUpdateDelayFast` in milliseconds, its
/// `PackUnpackVariationFactor`, its `RegularCashAmount`, `VeteranCashAmount`, `EliteCashAmount`
/// and `HeroicCashAmount`, by veterancy from regular, and its `XpPerCashUpdate`, each 0 by
/// default.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct InternetHack {
    pub(crate) unpack_ms: i32,
    pub(crate) pack_ms: i32,
    pub(crate) cash_delay_ms: i32,
    pub(crate) cash_delay_fast_ms: i32,
    pub(crate) variation: f32,
    pub(crate) cash: [i32; 4],
    pub(crate) xp_per_cash: i32,
}

/// An `AutoDepositUpdate`: its `DepositTiming` in milliseconds, its `DepositAmount`, its
/// `InitialCaptureBonus`, and whether it pays money, its `ActualMoney`, yes by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AutoDeposit {
    pub(crate) timing_ms: i32,
    pub(crate) amount: i32,
    pub(crate) capture: i32,
    pub(crate) actual_money: bool,
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

/// A `SpawnBehavior`: its `SpawnNumber`, whether it spawns once, its `OneShot`, its
/// `SpawnReplaceDelay` in milliseconds, its `SpawnTemplateName`s, in order, whether its spawner's
/// health is its spawns', its `AggregateHealth`, and whether they act on their own, its
/// `SlavesHaveFreeWill`, each false by default, as `SpawnBehaviorModuleData` sets them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct IniSpawn {
    pub(crate) number: i32,
    pub(crate) one_shot: bool,
    pub(crate) replace_ms: i32,
    pub(crate) templates: Vec<String>,
    pub(crate) aggregate_health: bool,
    pub(crate) free_will: bool,
}

impl IniSpawn {
    /// Whether its spawner leads its spawns, as a mob's nexus does: it lives by them, and they
    /// take its orders (09-zero-hour.md, The import converts a mob).
    pub(crate) const fn leads(&self) -> bool {
        self.aggregate_health && !self.free_will
    }
}

/// A behavior module whose fields supply reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SupplyModule {
    Dock,
    Gatherer,
    Spawn,
    Deposit,
    Hacker,
}

impl IniSupply {
    /// Opens the behavior module `module`, of the name its line gives in lowercase: the one of
    /// those supply reads, which starts as the game starts it; none for another, or for a
    /// `SupplyCenterDockUpdate` or an `OCLUpdate`, whose fields supply does not read.
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
            "oclupdate" => {
                self.creates_over_time = true;
                None
            }
            "hackinternetaiupdate" => {
                self.hacker = Some(InternetHack::default());
                Some(SupplyModule::Hacker)
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
            "autodepositupdate" => {
                self.deposit = Some(AutoDeposit {
                    timing_ms: 0,
                    amount: 0,
                    capture: 0,
                    actual_money: true,
                });
                Some(SupplyModule::Deposit)
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
            SupplyModule::Deposit => {
                let deposit = self.deposit.as_mut().expect("a deposit's module is open");
                match word {
                    "deposittiming" => deposit.timing_ms = at.int()?,
                    "depositamount" => deposit.amount = at.int()?,
                    "initialcapturebonus" => deposit.capture = at.int()?,
                    "actualmoney" => deposit.actual_money = at.yes()?,
                    _ => {}
                }
            }
            SupplyModule::Hacker => {
                let hacker = self.hacker.as_mut().expect("a hacker's module is open");
                match word {
                    "unpacktime" => hacker.unpack_ms = at.int()?,
                    "packtime" => hacker.pack_ms = at.int()?,
                    "cashupdatedelay" => hacker.cash_delay_ms = at.int()?,
                    "cashupdatedelayfast" => hacker.cash_delay_fast_ms = at.int()?,
                    "packunpackvariationfactor" => hacker.variation = at.real()?,
                    "regularcashamount" => hacker.cash[0] = at.int()?,
                    "veterancashamount" => hacker.cash[1] = at.int()?,
                    "elitecashamount" => hacker.cash[2] = at.int()?,
                    "heroiccashamount" => hacker.cash[3] = at.int()?,
                    "xppercashupdate" => hacker.xp_per_cash = at.int()?,
                    _ => {}
                }
            }
            SupplyModule::Spawn => {
                let spawn = self.spawns.last_mut().expect("a spawn's module is open");
                match word {
                    "spawnnumber" => spawn.number = at.int()?,
                    "oneshot" => spawn.one_shot = at.yes()?,
                    "spawnreplacedelay" => spawn.replace_ms = at.int()?,
                    "aggregatehealth" => spawn.aggregate_health = at.yes()?,
                    "slaveshavefreewill" => spawn.free_will = at.yes()?,
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

    /// The spawns its object keeps: its first `SpawnBehavior` of no `OneShot` that names a
    /// template (09-zero-hour.md, A kept `SpawnBehavior` converts to spawns).
    pub(crate) fn kept(&self) -> Option<&IniSpawn> {
        self.spawns
            .iter()
            .find(|spawn| !spawn.one_shot && !spawn.templates.is_empty())
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
