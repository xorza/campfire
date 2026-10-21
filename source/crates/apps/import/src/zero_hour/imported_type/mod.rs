use campfire_combat::{CombatData, CombatPatch, TurretData, TurretPatch};
use campfire_common::DeclaredName;
use campfire_declarations::SectionWriter;
use campfire_geometry::BodyBox;
use campfire_math::Num;
use campfire_mode::{BountyData, Pays};
use campfire_projectiles::{CurveData, ProjectileData, ProjectileHits};
use campfire_stats::{StatValue, StatsData, TypePoolsData};
use campfire_units::{BodyForm, CollisionData, TypeTagsData};
use campfire_values::{EngineStat, FilterData, Stat};

use crate::zero_hour::error::TypeError;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::locomotor_ini::LocomotorIni;
use crate::zero_hour::format::ini::object_ini::fight_data::BodyKind;
use crate::zero_hour::format::ini::object_ini::{GeometryShape, IniObject};
use crate::zero_hour::format::ini::weapon_ini::IniWeapon;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::weapon_drop::{self, WeaponDrop};

/// An object's unit type as the import writes it in `data/units.toml`  (09-zero-hour.md, Units move
/// by the generic backends with the original's numbers; 09-zero-hour.md, Only walkers and standing
/// structures have bodies; 09-zero-hour.md, Life is the original's; 09-zero-hour.md, Defeat by the
/// last building that counts): the speed and the turn rate of its first normal locomotor that
/// drives on the ground; its body: a circle of its bounding radius, at most `MAX_WALKER`, when it
/// drives, or, when it is a structure that is not small, a box of twice its radii for a box
/// geometry, else a circle of its radius, of its geometry's height (04-capabilities/00-overview.md,
/// A body has a height); the life of an active or a structure body, with its `combat` section of
/// the armor of its starting armor set; the tags of its kinds, of victory for a structure that
/// counts for one, and of a structure, a wall and infantry, which a shell's hits and a scatter
/// name; the turn rate of the turret of its AI module, which turns its weapons; and, of a life, its
/// `BuildCost` in money as the cost a kill's bounty takes its share of. Every other object has none
/// of these. What every type shares, a death that despawns and a turret's tolerance and slot kind,
/// is Zero Hour's rules data, `[defaults]`, which lies under each type's own (03-game-scripting.md,
/// Defaults under each unit type). A shell's type is a projectile alone (09-zero-hour.md, The
/// import converts shells, splash, scatter, pitch, turrets and slow weapons).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportedType {
    pools: Option<TypePoolsData>,
    tags: Option<TypeTagsData>,
    stats: StatsData,
    collision: Option<CollisionData>,
    /// The armor of its starting armor set, the one field of its `combat` the install gives; the
    /// engine's own load completes it from the rules data's defaults.
    combat: Option<CombatPatch>,
    /// Its turn rate, the one field of its `turret` the install gives, completed as `combat` is.
    turret: Option<TurretPatch>,
    bounty: Option<BountyData>,
    projectile: Option<ProjectileData>,
}

/// The player resource every cost is in.
pub(crate) const MONEY: &str = "money";

/// The mode's pool of life, and the stat of its maximum.
pub(crate) const LIFE: &str = "life";

/// The tag of a type that counts for a multiplayer victory (09-zero-hour.md, Defeat by the last
/// building that counts).
pub(crate) const VICTORY: &str = "counts_for_victory";

/// The tags of a structure, a wall and infantry, of the objects' `KindOf` and `FenceWidth`.
pub(crate) const STRUCTURE: &str = "structure";
pub(crate) const WALL: &str = "wall";
pub(crate) const INFANTRY: &str = "infantry";

/// The widest a driving type's circle is, 25 m: the original's largest pathfinding footprint, 2
/// cells of 10 m on each side of a center cell (`MAX_RADIUS` of `getRadiusAndCenter`), which our
/// one body for collision and pathfinding takes (09-zero-hour.md, Only walkers and standing
/// structures have bodies).
const MAX_WALKER: f32 = 25.0;

impl ImportedType {
    /// The unit type of `object`, its locomotors found in `locomotors`; an error for a locomotor
    /// no file defines, as the game refuses one, a number past what the engine holds, or a body
    /// or a turret a unit type's load refuses, as the engine's types refuse them.
    pub(crate) fn of(object: &IniObject, locomotors: &LocomotorIni) -> Result<Self, TypeError> {
        let mut drive = None;
        for name in object.locomotors() {
            let locomotor = locomotors
                .get(name)
                .ok_or_else(|| TypeError::NoLocomotor(name.clone()))?;
            if drive.is_none() && locomotor.drives() {
                drive = Some(locomotor);
            }
        }
        let geometry = object.geometry();
        let form = match drive {
            Some(_) => Some(BodyForm::circle(num(geometry
                .bounding_circle()
                .min(MAX_WALKER))?)),
            None if object.is_structure() && !geometry.small => Some(match geometry.shape {
                GeometryShape::Box => {
                    BodyForm::box_sized([num(2.0 * geometry.major)?, num(2.0 * geometry.minor)?])
                }
                GeometryShape::Sphere | GeometryShape::Cylinder => {
                    BodyForm::circle(num(geometry.major)?)
                }
            }),
            None => None,
        };
        let collision = match form {
            Some(form) => {
                let height = num(geometry.height())?;
                let form = form.and_then(|form| form.raised(height));
                let form = form.ok_or(TypeError::Body)?;
                Some(CollisionData { form, layer: None })
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
            stats.push((Self::life_stat(), StatValue::flat(life)));
        }
        let combat = life.map(|_| CombatPatch {
            armor: fight
                .starting_armor()
                .map(|armor| ImportName::latin(armor).declared()),
            ..CombatPatch::default()
        });
        let kinds = [
            (object.counts_for_victory(), VICTORY),
            (object.is_structure(), STRUCTURE),
            (object.is_wall(), WALL),
            (object.is_infantry(), INFANTRY),
        ];
        let tags: Vec<DeclaredName> = kinds
            .into_iter()
            .filter(|&(holds, _)| holds)
            .map(|(_, tag)| declared(tag))
            .collect();
        // A turret that turns no weapon, or does not turn, aims nothing the engine's turret aims.
        let turret = fight
            .turret
            .filter(|turret| turret.slots.contains(&true) && turret.turn_rate > 0.0)
            .map(|turret| {
                let turn_rate = num(turret.turn_rate)?;
                let turns = turn_rate > Num::ZERO;
                turns
                    .then(|| TurretPatch {
                        turn_rate: Some(turn_rate),
                        ..TurretPatch::default()
                    })
                    .ok_or(TypeError::Turret)
            })
            .transpose()?;
        let cost = object.build().cost;
        let bounty = (life.is_some() && cost > 0).then(|| BountyData {
            resource: declared(MONEY),
            pays: Pays::Cost(u32::from(cost)),
        });
        Ok(Self {
            pools: life.map(|_| TypePoolsData(vec![declared(LIFE)])),
            tags: (!tags.is_empty()).then_some(TypeTagsData(tags)),
            stats: stats.into_iter().collect(),
            collision,
            combat,
            turret,
            bounty,
            projectile: None,
        })
    }

    /// Writes each section it gives into `writer`, a unit type's table: whole, or the patch of
    /// what the install gives.
    pub(crate) fn write(&self, writer: &mut SectionWriter) {
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
        if let Some(combat) = &self.combat {
            writer.patch::<CombatData>(combat);
        }
        if let Some(turret) = &self.turret {
            writer.patch::<TurretData>(turret);
        }
        if let Some(bounty) = &self.bounty {
            writer.section(bounty);
        }
        if let Some(projectile) = &self.projectile {
            writer.section(projectile);
        }
    }

    /// The type of a shell, of `projectile` alone: it has no life, no body and no tag.
    pub(crate) fn shell(projectile: ProjectileData) -> Self {
        Self {
            pools: None,
            tags: None,
            stats: StatsData::default(),
            collision: None,
            combat: None,
            turret: None,
            bounty: None,
            projectile: Some(projectile),
        }
    }

    /// The projectile `weapon` fires, of `shell`, an object of a `DumbProjectileBehavior`: at its
    /// `WeaponSpeed`, twice its geometry's radius wide, detonating on what its
    /// `ProjectileCollidesWith` names, on the curve of its behavior; why the import drops the
    /// weapon, for a mask of what the engine's units are not, a number past what the engine
    /// holds, or a projectile the engine's type refuses.
    pub(crate) fn projectile(
        weapon: &IniWeapon,
        shell: &IniObject,
    ) -> Result<ProjectileData, WeaponDrop> {
        let flight = shell
            .fight()
            .shell
            .expect("a shell's object has a `DumbProjectileBehavior`");
        let collides = weapon.collides;
        // Shrubbery, projectiles and missiles are no units of the engine's.
        if collides.shrubbery
            || collides.projectiles
            || collides.small_missiles
            || collides.ballistic_missiles
        {
            return Err(WeaponDrop::Collides);
        }
        // As each player is a team of its own, a structure of no other player is an enemy's or a
        // neutral's, and one of its own an ally's; `ALLIES` and `ENEMIES` name the objects of
        // their relation alone, structures among them, and no neutral's.
        let filters = [
            (collides.allies, "allies"),
            (collides.enemies, "hostiles"),
            (collides.structures, "enemies:structure"),
            (collides.controlled_structures, "allies:structure"),
            (collides.walls, "all:wall"),
        ];
        let filters: Vec<FilterData> = filters
            .into_iter()
            .filter_map(|(holds, filter)| holds.then_some(filter))
            .map(|filter| FilterData::parse(filter).expect("the import's filters parse"))
            .collect();
        let number = |value: f32| game_float::to_num(value).map_err(|_| WeaponDrop::BadNumber);
        let life_ms = weapon_drop::game_ms(flight.lifespan_ms)?;
        let life_ms = u32::try_from(life_ms).map_err(|_| WeaponDrop::BadNumber)?;
        let curve = CurveData::new(
            [number(flight.first_indent)?, number(flight.first_height)?],
            [number(flight.second_indent)?, number(flight.second_height)?],
            number(flight.follow)?,
            Some(life_ms),
        )
        .map_err(|_| WeaponDrop::BadNumber)?;
        let hits = if filters.is_empty() {
            ProjectileHits::Nothing
        } else {
            ProjectileHits::Units(filters)
        };
        ProjectileData::builder(number(weapon.speed)?)
            .width(number(2.0 * shell.geometry().major)?)
            .hits(hits)
            .curve(curve)
            .build()
            .map_err(|_| WeaponDrop::BadNumber)
    }

    /// The stat of the most life.
    fn life_stat() -> Stat {
        Stat::named(LIFE).expect("the life stat's name is a stat's")
    }

    /// Its most life, of a type that lives.
    pub(crate) fn life(&self) -> Option<Num> {
        self.stats.value(&Self::life_stat()).map(|life| life.base)
    }

    /// Its armor, of a type that lives.
    pub(crate) fn armor(&self) -> Option<&DeclaredName> {
        self.combat.as_ref()?.armor.as_ref()
    }

    /// How far its body reaches from its unit's place along `x` and `z`, at `angle` degrees, as
    /// a map's load measures a placed box against its bounds; none for no body.
    pub(crate) fn reach(&self, angle: Num) -> Option<[Num; 2]> {
        let form = self.collision.as_ref()?.form;
        if let Some(radius) = form.circle_radius() {
            return Some([radius; 2]);
        }
        let size = form.box_size().expect("a form is a circle or a box");
        let body = BodyBox::new(size, angle).expect("a box's size alone decides whether it is one");
        Some(body.extent())
    }

    /// Whether its units walk: it drives, so it has a `move_speed`.
    pub(crate) fn walks(&self) -> bool {
        self.stats.walks()
    }

    /// Whether its body is a box, which a build places.
    pub(crate) fn is_box(&self) -> bool {
        self.collision
            .as_ref()
            .is_some_and(|collision| collision.form.is_box())
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

/// `name`, a name the import gives, as a declared name.
fn declared(name: &str) -> DeclaredName {
    DeclaredName::new(name).expect("the import's names are declared names")
}

#[cfg(test)]
pub(crate) mod samples {
    use campfire_math::Num;
    use campfire_projectiles::{CurveData, ProjectileData, ProjectileHits};
    use campfire_values::FilterData;

    /// The projectile of the fixture's `TankShell`, fired at `speed` m/s by a weapon of
    /// `STRUCTURES WALLS`: 2 m wide, its inner points at 50% and 90% of the way, 10 m up, its end
    /// following its victim by 100 m/s, for 10 s. 90% is 90 ÷ 100 in `f32`, 0.89999997615…, the
    /// bits 15,099,494 · 2⁻²⁴ of a `Num`.
    pub(crate) fn shell_projectile(speed: i64) -> ProjectileData {
        let filters =
            ["enemies:structure", "all:wall"].map(|text| FilterData::parse(text).unwrap());
        let curve = CurveData::new(
            [Num::HALF, Num::int(10)],
            [Num::from_bits(15_099_494), Num::int(10)],
            Num::int(100),
            Some(10_000),
        )
        .unwrap();
        ProjectileData::builder(Num::int(speed))
            .width(Num::int(2))
            .hits(ProjectileHits::Units(filters.into()))
            .curve(curve)
            .build()
            .unwrap()
    }
}

#[cfg(test)]
mod tests;
