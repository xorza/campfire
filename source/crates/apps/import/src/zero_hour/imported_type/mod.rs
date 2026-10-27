use std::num::NonZeroU32;

use campfire_airfields::{SortieData, StrandedData};
use campfire_combat::{
    CombatData, CombatPatch, OnDeath, TaggedArmorData, TurretData, TurretPatch, TurretPitchData,
};
use campfire_common::DeclaredName;
use campfire_declarations::SectionWriter;
use campfire_geometry::BodyBox;
use campfire_math::{Num, Vec3};
use campfire_mode::{BountyData, IncomeData, IncomePays, Pays};
use campfire_navigation::{FlightData, Gait, UnitNavigationData, WingsData};
use campfire_orders::AcquirePatch;
use campfire_projectiles::ProjectileData;
use campfire_stats::{StatValue, StatsData, TypePoolsData};
use campfire_units::{BodyShape, CollisionData, TypeTagsData, VariationData, VariationsData};
use campfire_values::{EngineStat, Stat};
use campfire_vision::{Sight, VisionData};

use crate::zero_hour::error::TypeError;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::damage_type::DamageType;
use crate::zero_hour::format::ini::locomotor_ini::Locomotor;
use crate::zero_hour::format::ini::object_ini::fight_data::{BodyKind, IniTurretPitch};
use crate::zero_hour::format::ini::object_ini::ini_supply::IniSpawn;
use crate::zero_hour::format::ini::object_ini::{GeometryShape, IniObject};
use crate::zero_hour::game_time;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::model_bones::ModelBones;
use crate::zero_hour::rules_names::RulesNames;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// An object's unit type as the import writes it in `data/units.toml`  (09-zero-hour.md, Units move
/// by the generic backends with the original's numbers; 09-zero-hour.md, Only walkers and standing
/// structures have bodies; 09-zero-hour.md, Life is the original's; 09-zero-hour.md, Defeat by the
/// last building that counts): the speed and the turn rate of its first normal locomotor that
/// drives on the ground or hovers, and of one that hovers its flight on the air layer
/// (09-zero-hour.md, The import flies a hovering locomotor); its body: a circle of its bounding
/// radius, at most `MAX_WALKER`, when it drives, or, when it is a structure that is not small, a
/// box of twice its radii for a box geometry, else a circle of its radius, of its geometry's height
/// (04-capabilities/00-overview.md, A body has a height); the life of an active or a structure
/// body, with its `combat` section of the armor of its starting armor set and, as its `armors`, the
/// one of its armor set of `PLAYER_UPGRADE` while its unit holds the tag of that flag
/// (09-zero-hour.md, Zero Hour's upgrade flags are tags and stats); the tags of its kinds, of
/// victory for a structure that counts for one, and of a structure, a wall and infantry, which a
/// projectile's hits and a scatter name; the turn rate of the turret of its AI module, which turns
/// its weapons; and, of a life, its `BuildCost` in money as the cost a kill's bounty takes its
/// share of; and the money its `AutoDepositUpdate` pays over time, a capture's bonus among it
/// (04-capabilities/games.md, Income over time); and of one that drives, its `navigation`: it
/// drives as a vehicle of `KindOf` that does not hover and steps as any other, at a weight of its
/// circle's radius squared, as a body's area weighs it (04-capabilities/navigation.md, A unit moves
/// by its kind of motion; 04-capabilities/navigation.md, A unit has a weight). Every other object
/// has none of these. What every type shares, a death that despawns and a turret's tolerance and
/// slot kind, is Zero Hour's rules data, `[defaults]`, which lies under each type's own
/// (03-game-scripting.md, Defaults under each unit type). A fired projectile's type is a projectile
/// alone (09-zero-hour.md, The import converts shells, splash, scatter, pitch, turrets and slow
/// weapons; 09-zero-hour.md, A missile converts to a steered projectile).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportedType {
    /// Its most life, of a type that lives, which its `stats` hold too.
    life: Option<Num>,
    pools: Option<TypePoolsData>,
    tags: Option<TypeTagsData>,
    stats: StatsData,
    collision: Option<CollisionData>,
    /// The body of each of its build variations, in their order, of a type that varies, in place
    /// of its own `collision` (04-capabilities/00-overview.md, A unit type may vary its units).
    variations: Option<VariationsData>,
    /// The armor of its starting armor set, and of its upgrade's, the fields of its `combat` the
    /// install gives; the engine's own load completes it from the rules data's defaults.
    combat: Option<CombatPatch>,
    /// Its `turret`, of the fields the install gives, completed as `combat` is, and the weapon
    /// slots it carries.
    turret: Option<ImportedTurret>,
    bounty: Option<BountyData>,
    income: Option<IncomeData>,
    projectile: Option<ProjectileData>,
    navigation: Option<UnitNavigationData>,
    /// The flight of a type whose locomotor hovers (09-zero-hour.md, The import flies a hovering
    /// locomotor).
    flight: Option<FlightData>,
    /// The sortie of a jet, which an airfield parks (09-zero-hour.md, The import flies a winged
    /// locomotor).
    sortie: Option<SortieData>,
    /// Whether it leads its spawns, so it has no weapon, body, walk or life of its own.
    leads: bool,
    /// The period of its AI module's idle scan, and every hostile unit for one that attacks
    /// buildings, as the install gives them, completed from the rules data's defaults, for a type
    /// of a weapon (09-zero-hour.md, A scan follows `AutoAcquireEnemiesWhenIdle`).
    acquire: Option<AcquirePatch>,
    /// How far it sees, of a type whose range clears the shroud (09-zero-hour.md, Zero Hour sees by
    /// the vision capability).
    vision: Option<VisionData>,
}

/// A type's turret: the fields of its `turret` the install gives, and the weapon slots it carries,
/// `ControlledWeaponSlots`, by their places, `PRIMARY` first.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ImportedTurret {
    patch: TurretPatch,
    slots: [bool; 3],
}

/// A type's weapons, as its slots of the kind `weapon` hold them, `all`, and as each of its weapon
/// sets' slots does, `PRIMARY` first, `by_slot`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TypeWeapons<'a> {
    pub(crate) all: &'a [DeclaredName],
    pub(crate) by_slot: &'a [Vec<DeclaredName>],
}

/// The widest a driving type's circle is, 25 m: the original's largest pathfinding footprint, 2
/// cells of 10 m on each side of a center cell (`MAX_RADIUS` of `getRadiusAndCenter`), which our
/// one body for collision and pathfinding takes (09-zero-hour.md, Only walkers and standing
/// structures have bodies).
const MAX_WALKER: f32 = 25.0;

impl ImportedType {
    /// The unit type of `object`, its locomotors found in `rules`, its names those `rules`
    /// give; an error for a locomotor no file defines, as the game refuses one, a number past what
    /// the engine holds, or a body or a turret a unit type's load refuses, as the engine's types
    /// refuse them. An object of build variations takes its first variation's type, of the body of
    /// each.
    pub(crate) fn of(object: &IniObject, rules: &ZeroHourRules) -> Result<Self, TypeError> {
        match rules.objects.variations_of(object) {
            Some(variations) => Self::varied(variations, rules),
            None => Self::plain(object, rules),
        }
    }

    /// The type of the first of `variations`, of the body of each, which every other part of
    /// theirs is alike in; of no body, of none; an error for a variation of no body beside one
    /// of a body.
    fn varied<'r>(
        mut variations: impl Iterator<Item = &'r IniObject>,
        rules: &ZeroHourRules,
    ) -> Result<Self, TypeError> {
        let first = variations.next().expect("a parent's variations are some");
        let mut varied = Self::plain(first, rules)?;
        let Some(collision) = varied.collision.take() else {
            return Ok(varied);
        };
        let mut bodies = vec![VariationData { collision }];
        for variation in variations {
            let collision = Self::plain(variation, rules)?.collision;
            let collision = collision.ok_or(TypeError::Body)?;
            bodies.push(VariationData { collision });
        }
        varied.variations = Some(VariationsData(bodies));
        Ok(varied)
    }

    /// The type of `object` alone.
    fn plain(object: &IniObject, rules: &ZeroHourRules) -> Result<Self, TypeError> {
        let names = &rules.names;
        let mut drive = None;
        for name in object.locomotors() {
            let locomotor = rules
                .locomotors
                .get(name)
                .ok_or_else(|| TypeError::NoLocomotor(name.clone()))?;
            let jet = locomotor.wings() && object.fight().jet.is_some();
            if drive.is_none() && (locomotor.drives() || locomotor.hovers() || jet) {
                drive = Some(locomotor);
            }
        }
        let flight = drive
            .filter(|locomotor| locomotor.hovers() || locomotor.wings())
            .map(|locomotor| Self::flight(locomotor, rules.game_data.gravity))
            .transpose()?;
        let sortie = drive
            .filter(|locomotor| locomotor.wings())
            .map(|_| Self::sortie_of(object, rules))
            .transpose()?;
        if drive.is_some() && object.supply().kept().is_some_and(IniSpawn::leads) {
            return Ok(Self::leader(object, names));
        }
        let geometry = object.geometry();
        let walker = drive
            .map(|_| num(geometry.bounding_circle().min(MAX_WALKER)))
            .transpose()?;
        let navigation = walker
            .map(|radius| {
                // A hover moves any way, as a soldier steps.
                let gait = if object.is_vehicle() && flight.is_none() {
                    Gait::Drive
                } else {
                    Gait::Step
                };
                UnitNavigationData::new(gait, radius * radius).ok_or(TypeError::Body)
            })
            .transpose()?;
        let shape = match walker {
            Some(radius) => Some(BodyShape::circle(radius)),
            None if object.is_structure() && !geometry.small => Some(match geometry.shape {
                GeometryShape::Box => {
                    BodyShape::box_sized([num(2.0 * geometry.major)?, num(2.0 * geometry.minor)?])
                }
                GeometryShape::Sphere | GeometryShape::Cylinder => {
                    BodyShape::circle(num(geometry.major)?)
                }
            }),
            None => None,
        };
        let collision = match shape {
            Some(shape) => {
                let height = num(geometry.height())?;
                let shape = shape.and_then(|shape| shape.raised(height));
                let shape = shape.ok_or(TypeError::Body)?;
                let layer = flight.map(|_| names.air.clone());
                Some(CollisionData { shape, layer })
            }
            None => None,
        };
        let mut stats = Vec::new();
        if let Some(locomotor) = drive {
            let speed = StatValue::flat(num(locomotor.speed)?);
            stats.push((Stat::Engine(EngineStat::MoveSpeed), speed));
            let turn_rate = StatValue::flat(num(locomotor.turn_rate)?);
            stats.push((Stat::Engine(EngineStat::TurnRate), turn_rate));
        }
        let fight = object.fight();
        let body = fight
            .body
            .filter(|body| matches!(body.kind, BodyKind::Active | BodyKind::Structure));
        let life = body
            .filter(|body| body.max > 0.0)
            .map(|body| num(body.max))
            .transpose()?;
        if let Some(life) = life {
            stats.push((names.life_stat.clone(), StatValue::flat(life)));
        }
        let armor = fight
            .starting_armor()
            .map(|armor| ImportName::latin(armor).declared())
            .transpose()?;
        let upgraded = fight
            .upgrade_armor()
            .filter(|&upgraded| fight.starting_armor() != Some(upgraded))
            .map(|upgraded| ImportName::latin(upgraded).declared())
            .transpose()?;
        let armors = upgraded.map(|armor| {
            vec![TaggedArmorData {
                when: names.armor_upgraded.clone(),
                armor,
            }]
        });
        // A death keeps the object for the delay of the slow death a normal death of it most
        // likely takes (04-capabilities/combat.md, A death keeps its unit for its animation).
        let kept = object.slow_death().map(|death| {
            let ms = game_time::of_duration(death.delay_ms).map_err(|_| TypeError::DeathDelay)?;
            u32::try_from(ms).map_err(|_| TypeError::DeathDelay)
        });
        let on_death = kept
            .transpose()?
            .and_then(NonZeroU32::new)
            .map(|ms| OnDeath::Keep { ms });
        let combat = life.map(|_| CombatPatch {
            on_death,
            armor,
            armors,
        });
        // A turret that turns no weapon, or does not turn, aims nothing the engine's turret aims.
        let turret = fight
            .turret
            .filter(|turret| turret.slots.contains(&true) && turret.turn_rate > 0.0)
            .map(|turret| {
                let turn_rate = num(turret.turn_rate)?;
                let turns = turn_rate > Num::ZERO;
                let pitch = turret
                    .allows_pitch
                    .then(|| Self::pitch(turret.pitch))
                    .transpose()?
                    .flatten();
                let patch = TurretPatch {
                    turn_rate: Some(turn_rate),
                    pitch,
                    ..TurretPatch::default()
                };
                turns
                    .then_some(ImportedTurret {
                        patch,
                        slots: turret.slots,
                    })
                    .ok_or(TypeError::Turret)
            })
            .transpose()?;
        let scan = fight.scan;
        let acquire = scan
            .idle
            .then(|| {
                let scan_ms = game_time::of_ms(scan.check_rate_ms);
                let scan_ms = scan_ms.map_err(|_| TypeError::Number)?;
                Ok::<_, TypeError>(AcquirePatch {
                    scan_ms: Some(scan_ms),
                    targets: scan.attack_buildings.then(|| vec![names.hostiles.clone()]),
                })
            })
            .transpose()?;
        let cost = object.build().cost;
        let bounty = (life.is_some() && cost > 0).then(|| BountyData {
            resource: names.money.clone(),
            pays: Pays::Cost(u32::from(cost)),
        });
        let income = Self::income(object, names)?;
        Ok(Self {
            life,
            pools: life.map(|_| TypePoolsData(vec![names.life.clone()])),
            tags: Self::tags(object, names),
            stats: stats.into_iter().collect(),
            collision,
            variations: None,
            combat,
            turret,
            bounty,
            income,
            projectile: None,
            navigation,
            flight,
            sortie,
            acquire,
            leads: false,
            vision: Self::vision(object)?,
        })
    }

    /// The flight of a hovering `locomotor` under the game's `gravity`, in units a second
    /// squared, below 0: at its `PreferredHeight`, an air target past its
    /// `AirborneTargetingHeight`, or never for none, and climbing at the mean speed of its climb
    /// from rest to that height, its lift net of gravity lifting it and gravity braking it, as
    /// `Locomotor::calcLiftToUseAtPt` solves the equation of motion: √(h ÷ 2(1/a + 1/g)), so it
    /// reaches its height in the original's time (09-zero-hour.md, The import flies a hovering
    /// locomotor).
    fn flight(locomotor: &Locomotor, gravity: f32) -> Result<FlightData, TypeError> {
        let height = locomotor.preferred_height;
        let braking = -gravity;
        let lifting = locomotor.lift + gravity;
        if !(height > 0.0 && braking > 0.0 && lifting > 0.0) {
            return Err(TypeError::Flight);
        }
        let climb = (height / (2.0 * (1.0 / lifting + 1.0 / braking))).sqrt();
        let airborne = match locomotor.airborne_height {
            Some(airborne) => num(airborne as f32)?,
            None => Vec3::BOUND,
        };
        let flight =
            FlightData::new(num(height)?, num(climb)?, airborne).ok_or(TypeError::Flight)?;
        if !locomotor.wings() {
            return Ok(flight);
        }
        let wings = WingsData::new(num(locomotor.min_speed)?, num(locomotor.circling_radius)?);
        Ok(flight.winged(wings.ok_or(TypeError::Flight)?))
    }

    /// The sortie of the jet `object`: its `JetAIUpdate`'s `TakeoffPause` and
    /// `ReturnToBaseIdleTime`, none for 0, its `OutOfAmmoDamagePerSecond` as `unresistable` damage,
    /// as Zero Hour deals it, none for 0, and the speed of its first taxi locomotor that drives
    /// (09-zero-hour.md, The import flies a winged locomotor); an error for a jet of none, or of a
    /// loss past its whole life a second.
    fn sortie_of(object: &IniObject, rules: &ZeroHourRules) -> Result<SortieData, TypeError> {
        let jet = object.fight().jet.expect("a jet has its AI's sortie");
        let taxi = object
            .taxi_locomotors()
            .iter()
            .filter_map(|name| rules.locomotors.get(name))
            .find(|locomotor| locomotor.drives())
            .ok_or(TypeError::Sortie)?;
        let ms = |ms: i32| u32::try_from(ms).map_err(|_| TypeError::Sortie);
        let stranded = match num(jet.out_of_ammo_damage)? {
            share if share <= Num::ZERO => None,
            share if share <= Num::ONE => Some(StrandedData {
                share,
                kind: DamageType::Unresistable.kind(),
            }),
            _ => return Err(TypeError::Sortie),
        };
        Ok(SortieData {
            pause_ms: ms(jet.takeoff_pause_ms)?,
            taxi_speed: num(taxi.speed)?,
            idle_ms: Some(ms(jet.idle_ms)?).filter(|&ms| ms > 0),
            stranded,
        })
    }

    /// The type of `object`, a walker that leads its spawns: its tags alone, as it has no life,
    /// body, walk, turret or scan of its own, its health theirs (09-zero-hour.md, The import
    /// converts a mob).
    fn leader(object: &IniObject, names: &RulesNames) -> Self {
        Self {
            life: None,
            pools: None,
            tags: Self::tags(object, names),
            stats: StatsData::default(),
            collision: None,
            variations: None,
            combat: None,
            turret: None,
            bounty: None,
            income: None,
            projectile: None,
            navigation: None,
            flight: None,
            sortie: None,
            acquire: None,
            leads: true,
            vision: None,
        }
    }

    /// The sight of `object`, of its range that clears the shroud; none for one of no range above
    /// 0, which reveals nothing, as `Object::look` skips it.
    fn vision(object: &IniObject) -> Result<Option<VisionData>, TypeError> {
        let range = object.sight_range();
        if range <= 0.0 {
            return Ok(None);
        }
        let sight = Sight::new(num(range)?).expect("a range above 0 is no negative sight");
        Ok(Some(VisionData { sight }))
    }

    /// The tags of `object`'s kinds that the rules data declares, and its radar's, of one made with
    /// it.
    fn tags(object: &IniObject, names: &RulesNames) -> Option<TypeTagsData> {
        let kinds = [
            (object.counts_for_victory(), &names.counts_for_victory),
            (object.is_structure(), &names.structure),
            (object.is_wall(), &names.wall),
            (object.is_base_defense(), &names.base_defense),
            (object.is_infantry(), &names.infantry),
            (object.makes_radar(), &names.radar),
        ];
        let tags: Vec<DeclaredName> = kinds
            .into_iter()
            .filter(|&(holds, _)| holds)
            .map(|(_, tag)| tag.clone())
            .collect();
        (!tags.is_empty()).then_some(TypeTagsData(tags))
    }

    /// The income of `object`'s `AutoDepositUpdate` in money: its amount each period of its
    /// timing, and its capture's bonus; none for a deposit of no money, of no amount, or of no
    /// time, which pays each frame (`AutoDepositUpdate::update`).
    fn income(object: &IniObject, names: &RulesNames) -> Result<Option<IncomeData>, TypeError> {
        let Some(deposit) = object.supply().deposit else {
            return Ok(None);
        };
        let period_ms = game_time::of_ms(deposit.timing_ms).map_err(|_| TypeError::Number)?;
        let Ok(amount) = u32::try_from(deposit.amount) else {
            return Ok(None);
        };
        if !deposit.actual_money || amount == 0 || period_ms == 0 {
            return Ok(None);
        }
        Ok(Some(IncomeData {
            resource: names.money.clone(),
            pays: IncomePays::Amount(amount),
            period_ms,
            capture: u32::try_from(deposit.capture).unwrap_or(0),
        }))
    }

    /// The pitch of a turret of `AllowsPitch` (04-capabilities/combat.md, A turret may pitch):
    /// its fire pitch when above 0, as `TurretAIAimTurretState::update` takes one; none for a
    /// pitch rate not above 0, which never turns the pitch; an error for an angle past a quarter
    /// turn either way.
    fn pitch(pitch: IniTurretPitch) -> Result<Option<TurretPitchData>, TypeError> {
        if pitch.rate <= 0.0 {
            return Ok(None);
        }
        let fire = (pitch.fire > 0.0).then(|| num(pitch.fire)).transpose()?;
        let data =
            TurretPitchData::new(num(pitch.rate)?, fire, num(pitch.least)?, num(pitch.rest)?);
        data.map(Some).ok_or(TypeError::Turret)
    }

    /// Its idle scan, which a type of a weapon gives.
    /// The places among `weapons` of those of the weapon slots `slots` names, as Zero Hour's
    /// turret aims the slots its `ControlledWeaponSlots` names
    /// (`AIUpdateInterface::getWhichTurretForWeaponSlot`); none when they are every one, as a
    /// turret carries every slot of its kind by default (04-capabilities/combat.md, A unit type may
    /// have a turret). A weapon of a slot it names and of one it does not is carried.
    fn places(slots: [bool; 3], weapons: TypeWeapons<'_>) -> Option<Vec<u8>> {
        let carried = |weapon: &DeclaredName| {
            let named = weapons.by_slot.iter().zip(slots);
            named
                .into_iter()
                .any(|(of_slot, named)| named && of_slot.contains(weapon))
        };
        let places: Vec<u8> = (0_u8..)
            .zip(weapons.all)
            .filter(|(_, weapon)| carried(weapon))
            .map(|(place, _)| place)
            .collect();
        (places.len() < weapons.all.len()).then_some(places)
    }

    pub(crate) const fn acquire(&self) -> Option<&AcquirePatch> {
        self.acquire.as_ref()
    }

    /// Writes each section it gives into `writer`, a unit type's table: whole, or the patch of
    /// what the install gives, its turret turning and pitching about the pivots of `bones`, the
    /// bones its model gives.
    pub(crate) fn write(
        &self,
        writer: &mut SectionWriter,
        bones: Option<&ModelBones>,
        weapons: TypeWeapons<'_>,
    ) {
        if let Some(pools) = &self.pools {
            writer.section(pools);
        }
        if let Some(tags) = &self.tags {
            writer.section(tags);
        }
        if !self.stats.is_empty() {
            writer.section(&self.stats);
        }
        if let Some(collision) = &self.collision {
            writer.section(collision);
        }
        if let Some(variations) = &self.variations {
            writer.section(variations);
        }
        if let Some(combat) = &self.combat {
            writer.patch::<CombatData>(combat);
        }
        if let Some(ImportedTurret { patch, slots }) = &self.turret {
            let pivot = bones.and_then(|bones| bones.pivot);
            let pitch_pivot = bones.and_then(|bones| bones.pitch_pivot);
            let turret = TurretPatch {
                pivot,
                pitch: patch
                    .pitch
                    .map(|pitch| pitch.about(pitch_pivot.unwrap_or_default())),
                places: Self::places(*slots, weapons),
                ..patch.clone()
            };
            writer.patch::<TurretData>(&turret);
        }
        if let Some(bounty) = &self.bounty {
            writer.section(bounty);
        }
        if let Some(income) = &self.income {
            writer.section(income);
        }
        if let Some(projectile) = &self.projectile {
            writer.section(projectile);
        }
        if let Some(navigation) = &self.navigation {
            writer.section(navigation);
        }
        if let Some(sortie) = &self.sortie {
            writer.section(sortie);
        }
        if let Some(flight) = &self.flight {
            writer.section(flight);
        }
        if let Some(vision) = &self.vision {
            writer.section(vision);
        }
    }

    /// The type of a projectile, of `projectile` alone: it has no life, no body and no tag.
    pub(crate) fn of_projectile(projectile: ProjectileData) -> Self {
        Self {
            life: None,
            pools: None,
            tags: None,
            stats: StatsData::default(),
            collision: None,
            variations: None,
            combat: None,
            turret: None,
            bounty: None,
            income: None,
            projectile: Some(projectile),
            navigation: None,
            flight: None,
            sortie: None,
            acquire: None,
            leads: false,
            vision: None,
        }
    }

    /// Whether it leads its spawns, which carry out its orders and fight in its place.
    pub(crate) const fn leads(&self) -> bool {
        self.leads
    }

    /// Its most life, of a type that lives.
    pub(crate) const fn life(&self) -> Option<Num> {
        self.life
    }

    /// Whether it is a fired projectile's type, which holds its flight alone.
    pub(crate) const fn is_projectile(&self) -> bool {
        self.projectile.is_some()
    }

    /// Its armor, of a type that lives.
    pub(crate) fn armor(&self) -> Option<&DeclaredName> {
        self.combat.as_ref()?.armor.as_ref()
    }

    /// The armors it takes by its tags, of a type that lives.
    pub(crate) fn tagged_armors(&self) -> &[TaggedArmorData] {
        let combat = self.combat.as_ref();
        combat
            .and_then(|combat| combat.armors.as_deref())
            .unwrap_or_default()
    }

    /// How far its body reaches from its unit's place along `x` and `z`, at `angle` degrees, as
    /// a map's load measures a placed box against its bounds, of a type that varies the farthest
    /// of its variations' along each; none for no body.
    pub(crate) fn reach(&self, angle: Num) -> Option<[Num; 2]> {
        let reach = |collision: &CollisionData| {
            let shape = collision.shape;
            if let Some(radius) = shape.circle_radius() {
                return [radius; 2];
            }
            let size = shape.box_size().expect("a shape is a circle or a box");
            let body = BodyBox::new(size, angle);
            body.expect("a box's size alone decides whether it is one")
                .extent()
        };
        let varied = self.variations.iter().flat_map(|variations| &variations.0);
        let bodies = self
            .collision
            .iter()
            .chain(varied.map(|variation| &variation.collision));
        bodies
            .map(reach)
            .reduce(|far, reach| [0, 1].map(|axis| far[axis].max(reach[axis])))
    }

    /// Its sortie, of a jet an airfield parks.
    pub(crate) const fn sortie(&self) -> Option<&SortieData> {
        self.sortie.as_ref()
    }

    /// Whether its units walk: it drives, so it has a `move_speed`.
    pub(crate) fn walks(&self) -> bool {
        self.stats.walks()
    }

    /// Whether it has a body, a box or a circle, which a build places, of its own or of each
    /// variation.
    pub(crate) const fn has_body(&self) -> bool {
        self.collision.is_some() || self.variations.is_some()
    }

    /// Its speed in meters a second, when it drives.
    pub(crate) fn move_speed(&self) -> Option<Num> {
        let speed = self.stats.value(&Stat::Engine(EngineStat::MoveSpeed));
        speed.map(|speed| speed.base)
    }
}

/// `value` exactly as a `Num`; an error for one that is not finite, or past what a `Num` holds.
fn num(value: f32) -> Result<Num, TypeError> {
    game_float::to_num(value).map_err(|_| TypeError::Number)
}

#[cfg(test)]
mod tests;
