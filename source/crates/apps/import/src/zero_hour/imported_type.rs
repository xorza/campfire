use campfire_common::DeclaredName;
use campfire_geometry::BodyBox;
use campfire_math::Num;
use campfire_mode::{BountyData, Pays};
use campfire_projectiles::{CurveData, ProjectileData, ProjectileHits};
use campfire_stats::{StatValue, StatsData, TypePoolsData};
use campfire_units::{BodyForm, CollisionData, TypeTagsData};
use campfire_values::{EngineStat, FilterData, Stat};
use serde::Serialize;

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ImportedType {
    #[serde(skip_serializing_if = "Option::is_none")]
    pools: Option<TypePoolsData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<TypeTagsData>,
    #[serde(skip_serializing_if = "StatsData::is_empty")]
    stats: StatsData,
    #[serde(skip_serializing_if = "Option::is_none")]
    collision: Option<CollisionData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    combat: Option<WrittenCombat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    turret: Option<WrittenTurret>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bounty: Option<BountyData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    projectile: Option<ProjectileData>,
}

/// A type's `combat` as the import writes it: the armor of its starting armor set, the one field
/// the install gives; the engine's own load of what the import wrote completes it from the rules
/// data's defaults, and refuses a key its section does not read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct WrittenCombat {
    #[serde(skip_serializing_if = "Option::is_none")]
    armor: Option<DeclaredName>,
}

/// A type's `turret` as the import writes it: its turn rate in degrees a second, positive, the one
/// field the install gives, which the engine's own load completes as it does a `combat`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct WrittenTurret {
    turn_rate: Num,
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
        let combat = life.map(|_| WrittenCombat {
            armor: fight
                .starting_armor()
                .map(|armor| ImportName::latin(armor).declared()),
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
                    .then_some(WrittenTurret { turn_rate })
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
pub(crate) mod internals {
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
mod tests {
    use super::*;
    use crate::zero_hour::format::ini::locomotor_ini::internals::LOCOMOTORS;
    use crate::zero_hour::format::ini::object_ini::ObjectIni;
    use crate::zero_hour::format::ini::object_ini::internals::{DEFAULT_OBJECTS, OBJECTS};
    use crate::zero_hour::format::ini::weapon_ini::WeaponIni;
    use crate::zero_hour::imported_type::internals::shell_projectile;

    /// The objects of each kind the import's rule of bodies tells apart.
    const BODIES: &str = "Object Tank
  KindOf = VEHICLE
  Locomotor = SET_NORMAL TankTreads
  Locomotor = SET_NORMAL_UPGRADED JetWings
  Geometry = BOX
  GeometryMajorRadius = 15.0
  GeometryMinorRadius = 10.0
End
Object Jet
  Locomotor = SET_NORMAL None JetWings
  Geometry = SPHERE
  GeometryMajorRadius = 12.0
End
Object Hover
  Locomotor = SET_NORMAL JetWings TankTreads
  Geometry = CYLINDER
  GeometryMajorRadius = 6.5
  GeometryHeight = 20.0
End
Object Barracks
  KindOf = STRUCTURE
  Geometry = BOX
  GeometryMajorRadius = 60.0
  GeometryMinorRadius = 70.0
  GeometryHeight = 49.0
End
Object Tower
  KindOf = STRUCTURE
  Geometry = CYLINDER
  GeometryMajorRadius = 8.5
End
Object Mine
  KindOf = STRUCTURE
  Geometry = BOX
  GeometryMajorRadius = 3.0
  GeometryMinorRadius = 3.0
  GeometryIsSmall = Yes
End
ObjectReskin BigMine Mine
  GeometryIsSmall = No
End
Object Shrub
  KindOf = STRUCTURE
End
Object Rock
End
Object Lost
  Locomotor = SET_NORMAL NoSuchTreads
End
Object Train
  Locomotor = SET_NORMAL TankTreads
  Geometry = BOX
  GeometryMajorRadius = 112.0
  GeometryMinorRadius = 10.0
End
Object Flat
  KindOf = STRUCTURE
  Geometry = BOX
  GeometryMajorRadius = 5.0
  GeometryMinorRadius = 0.0
End
Object Huge
  KindOf = STRUCTURE
  GeometryMajorRadius = 2048.5
End
Object Dome
  KindOf = STRUCTURE
  Geometry = SPHERE
  GeometryMajorRadius = 5.0
  GeometryHeight = 9.0
End
Object Sunk
  KindOf = STRUCTURE
  Geometry = CYLINDER
  GeometryMajorRadius = 5.0
  GeometryHeight = -1.0
End
";

    #[test]
    fn an_objects_type_drives_by_its_ground_locomotor_and_has_its_body_of_its_height() {
        let mut ini = ObjectIni::default();
        ini.read(BODIES.as_bytes()).unwrap();
        let mut locomotors = LocomotorIni::default();
        locomotors.read(LOCOMOTORS.as_bytes()).unwrap();
        let of = |name: &str| ImportedType::of(ini.object(name.as_bytes()).unwrap(), &locomotors);
        let int = Num::int;
        let half = |x: i64| Num::from_bits(x << (Num::FRAC_BITS - 1));
        let body = |form: Option<BodyForm>, height| {
            let form = form.unwrap().raised(height).unwrap();
            Some(CollisionData { form, layer: None })
        };
        let circle = BodyForm::circle;
        let boxed = BodyForm::box_sized;
        let drives = |speed, turn_rate, radius, height| ImportedType {
            pools: None,
            tags: None,
            stats: StatsData::from([
                (Stat::Engine(EngineStat::MoveSpeed), StatValue::flat(speed)),
                (
                    Stat::Engine(EngineStat::TurnRate),
                    StatValue::flat(turn_rate),
                ),
            ]),
            collision: body(circle(radius), height),
            combat: None,
            turret: None,
            bounty: None,
            projectile: None,
        };
        let stands = |body| ImportedType {
            pools: None,
            tags: None,
            stats: StatsData::default(),
            collision: body,
            combat: None,
            turret: None,
            bounty: None,
            projectile: None,
        };
        // A structure stands of the tag of its kind.
        let structure = |body| ImportedType {
            tags: Some(TypeTagsData(vec![declared(STRUCTURE)])),
            ..stands(body)
        };
        // The tank drives on its treads, 30.5 m and 90° a second; a box of half extents 15 and
        // 10 bounds a circle of √(15² + 10²) = √325 = 18.02775637…, in `f32` the nearest of
        // 19 bits past the point, 9,451,736 · 2⁻¹⁹ = 18.0277557373…, exactly 302,455,552 · 2⁻²⁴
        // as a `Num`. Its geometry names no height, so it has the game's default, 1 m.
        let tank = of("Tank").unwrap();
        let radius = Num::from_bits(302_455_552);
        assert_eq!(tank, drives(half(61), int(90), radius, int(1)));
        assert_eq!(tank.move_speed(), Some(half(61)));
        // The jet only flies: no stats, and no body. The hover craft's first normal locomotor
        // flies, and its second drives, a cylinder's major radius 6.5, 20 m tall.
        assert_eq!(of("Jet").unwrap(), stands(None));
        assert_eq!(of("Jet").unwrap().move_speed(), None);
        assert_eq!(
            of("Hover").unwrap(),
            drives(half(61), int(90), half(13), int(20))
        );
        // A train's box bounds a circle of √(112² + 10²) ≈ 112.4 m, past the original's widest
        // pathfinding footprint: its circle is that footprint's 25 m.
        assert_eq!(
            of("Train").unwrap(),
            drives(half(61), int(90), int(25), int(1))
        );
        // A structure that is not small: a box of twice its radii, 120 × 140, 49 m tall; a
        // cylinder's circle of its radius 8.5; one of no geometry the game's sphere of radius 1,
        // whose height is its radius whatever its `GeometryHeight`, as `GeometryInfo::set` takes
        // it, and the sphere 5 m wide with a height of 9 m, 5 m tall.
        assert_eq!(
            of("Barracks").unwrap(),
            structure(body(boxed([int(120), int(140)]), int(49)))
        );
        assert_eq!(
            of("Tower").unwrap(),
            structure(body(circle(half(17)), int(1)))
        );
        assert_eq!(
            of("Shrub").unwrap(),
            structure(body(circle(int(1)), int(1)))
        );
        assert_eq!(of("Dome").unwrap(), structure(body(circle(int(5)), int(5))));
        // A small structure and an object of no kind block nothing; a reskin's own geometry
        // makes its mine large, a box of 6 × 6.
        assert_eq!(of("Mine").unwrap(), structure(None));
        assert_eq!(of("Rock").unwrap(), stands(None));
        assert_eq!(
            of("BigMine").unwrap(),
            structure(body(boxed([int(6), int(6)]), int(1)))
        );
        // A locomotor no file defines refuses the type, as the game refuses it.
        assert_eq!(
            of("Lost"),
            Err(TypeError::NoLocomotor("NoSuchTreads".to_owned()))
        );
        // A box of no width, a circle past the widest body's 2,048 m, and a height below 0, a
        // unit type's load refuses, so the import refuses them first.
        assert_eq!(of("Flat"), Err(TypeError::Body));
        assert_eq!(of("Huge"), Err(TypeError::Body));
        assert_eq!(of("Sunk"), Err(TypeError::Body));
    }

    /// The objects of each kind of life and of turret the import tells apart.
    const LIVING: &str = "Object Soldier
  KindOf = INFANTRY
  Body = ActiveBody ModuleTag_01
    MaxHealth = 180.0
    InitialHealth = 180.0
  End
  ArmorSet
    Conditions = PLAYER_UPGRADE
    Armor = ChemSuitArmor
  End
  ArmorSet
    Conditions = None
    Armor = HumanArmor
  End
End
Object Center
  KindOf = STRUCTURE MP_COUNT_FOR_VICTORY
  BuildCost = 2000
  Body = StructureBody ModuleTag_02
    MaxHealth = 5000.0
  End
End
Object Wall
  KindOf = MP_COUNT_FOR_VICTORY
  FenceWidth = 20
  Body = ActiveBody ModuleTag_03
    MaxHealth = 10
  End
  ArmorSet
    Conditions = None
    Armor = None
  End
End
Object Gunner
  Behavior = AIUpdateInterface ModuleTag_07
    Turret
      TurretTurnRate = 60
      ControlledWeaponSlots = SECONDARY
    End
  End
End
Object Idle
  Behavior = AIUpdateInterface ModuleTag_07
    Turret
      TurretTurnRate = 60
    End
  End
End
Object Fixed
  Behavior = AIUpdateInterface ModuleTag_07
    Turret
      TurretTurnRate = 0
      ControlledWeaponSlots = PRIMARY
    End
  End
End
Object Ghost
  BuildCost = 100
  Body = ImmortalBody ModuleTag_04
    MaxHealth = 100
  End
End
Object Hollow
  Body = ActiveBody ModuleTag_05
  End
End
ObjectReskin Veteran Soldier
  Body = ActiveBody ModuleTag_06
    MaxHealth = 9999
  End
End
";

    #[test]
    fn a_living_object_lives_by_its_body_and_takes_its_starting_armor_its_tags_and_its_turret() {
        let mut ini = ObjectIni::default();
        ini.read(LIVING.as_bytes()).unwrap();
        let of = |name: &str| {
            ImportedType::of(
                ini.object(name.as_bytes()).unwrap(),
                &LocomotorIni::default(),
            )
            .unwrap()
        };
        let lives = |life: i64, armor: Option<&str>, tags: Vec<&str>| ImportedType {
            pools: Some(TypePoolsData(vec![declared(LIFE)])),
            tags: (!tags.is_empty())
                .then(|| TypeTagsData(tags.into_iter().map(declared).collect())),
            stats: StatsData::from([(ImportedType::life_stat(), StatValue::flat(Num::int(life)))]),
            collision: None,
            combat: Some(WrittenCombat {
                armor: armor.map(|armor| DeclaredName::new(armor).unwrap()),
            }),
            turret: None,
            bounty: None,
            projectile: None,
        };
        // The soldier lives by its active body, 180, in the armor of its set of no condition,
        // the second it gives, by the name the mode declares; a reskin keeps its base's body,
        // as its table reads none.
        assert_eq!(
            of("Soldier"),
            lives(180, Some("humanarmor"), vec![INFANTRY])
        );
        assert_eq!(of("Veteran"), of("Soldier"));
        // A structure that counts for victory lives by its structure body, of no armor, and of
        // no geometry stands as the game's sphere of radius 1, 1 m tall, of the tags of both;
        // an object of the kind that is no structure, of an armor of `None`, counts for none,
        // and of a fence's width is a wall.
        let form = BodyForm::circle(Num::ONE)
            .unwrap()
            .raised(Num::ONE)
            .unwrap();
        // Its build cost of 2,000 is its bounty's cost, in money.
        let center = ImportedType {
            collision: Some(CollisionData { form, layer: None }),
            bounty: Some(BountyData {
                resource: declared(MONEY),
                pays: Pays::Cost(2000),
            }),
            ..lives(5000, None, vec![VICTORY, STRUCTURE])
        };
        assert_eq!(of("Center"), center);
        assert_eq!(of("Wall"), lives(10, None, vec![WALL]));
        // An immortal body, and a body of no most life, give no life, no combat, and no bounty,
        // whatever their cost.
        for name in ["Ghost", "Hollow"] {
            let typed = of(name);
            assert_eq!(
                (typed.pools, typed.combat, typed.bounty),
                (None, None, None),
                "{name}"
            );
        }
        // The gunner's turret turns 60° a second; a turret of no weapon, or of no turn, is none.
        let turret = WrittenTurret {
            turn_rate: Num::int(60),
        };
        assert_eq!(of("Gunner").turret, Some(turret));
        assert_eq!((of("Idle").turret, of("Fixed").turret), (None, None));
    }

    #[test]
    fn a_shell_flies_its_weapons_speed_on_its_curve_and_hits_what_its_mask_names() {
        let mut ini = ObjectIni::default();
        ini.read(DEFAULT_OBJECTS.as_bytes()).unwrap();
        ini.read(OBJECTS.as_bytes()).unwrap();
        let steep = "Object Steep\n  Behavior = DumbProjectileBehavior M\n    \
                     FirstPercentIndent = 150%\n  End\nEnd\n";
        ini.read(steep.as_bytes()).unwrap();
        let fired_by = |object: &str, lines: &str| {
            let mut weapons = WeaponIni::default();
            let text = format!("Weapon W\n  WeaponSpeed = 400\n{lines}End\n");
            weapons.read(text.as_bytes()).unwrap();
            ImportedType::projectile(
                weapons.get("W").unwrap(),
                ini.object(object.as_bytes()).unwrap(),
            )
        };
        let fired = |lines: &str| fired_by("TankShell", lines);
        let collides = |mask: &str| {
            fired(&format!("  ProjectileCollidesWith = {mask}\n")).map(|shell| shell.hits)
        };
        assert_eq!(
            fired("  ProjectileCollidesWith = STRUCTURES WALLS\n"),
            Ok(shell_projectile(400))
        );
        // The default mask, structures; an ally's, an enemy's, and one's own structures; none.
        let hits = |filters: &[&str]| {
            let filters = filters.iter().map(|text| FilterData::parse(text).unwrap());
            Ok(ProjectileHits::Units(filters.collect()))
        };
        assert_eq!(
            fired("").map(|shell| shell.hits),
            hits(&["enemies:structure"])
        );
        assert_eq!(
            collides("ALLIES ENEMIES CONTROLLED_STRUCTURES"),
            hits(&["allies", "hostiles", "allies:structure"])
        );
        assert_eq!(collides("NONE"), Ok(ProjectileHits::Nothing));
        // Shrubbery, projectiles and missiles are no units; a speed of 0, and a point past the
        // way's end, are no numbers the engine takes.
        for mask in [
            "SHRUBBERY",
            "PROJECTILES",
            "SMALL_MISSILES",
            "BALLISTIC_MISSILES",
        ] {
            assert_eq!(collides(mask), Err(WeaponDrop::Collides), "{mask}");
        }
        assert_eq!(fired("  WeaponSpeed = 0\n"), Err(WeaponDrop::BadNumber));
        assert_eq!(fired_by("Steep", ""), Err(WeaponDrop::BadNumber));
    }
}
