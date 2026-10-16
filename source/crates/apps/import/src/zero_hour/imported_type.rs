use campfire_geometry::{BodyBox, MAX_BOUND, Radius};
use campfire_math::Num;
use campfire_values::DeclaredName;
use serde::{Serialize, Serializer};

use crate::zero_hour::error::TypeError;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::locomotor_ini::LocomotorIni;
use crate::zero_hour::format::ini::object_ini::fight_data::BodyKind;
use crate::zero_hour::format::ini::object_ini::{GeometryShape, IniObject};
use crate::zero_hour::format::ini::weapon_ini::IniWeapon;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::weapon_drop::{self, WeaponDrop};

/// An object's unit type as the import writes it in `data/units.toml`  (12-zero-hour.md, Units move
/// by the generic backends with the original's numbers; 12-zero-hour.md, Only walkers and standing
/// structures have bodies; 12-zero-hour.md, Life is the original's; 12-zero-hour.md, Defeat by the
/// last building that counts): the speed and the turn rate of its first normal locomotor that
/// drives on the ground; its body: a circle of its bounding radius, at most `MAX_WALKER`, when it
/// drives, or, when it is a structure that is not small, a box of twice its radii for a box
/// geometry, else a circle of its radius, of its geometry's height (04-capabilities/00-overview.md,
/// A body has a height); the life of an active or a structure body, with its
/// `combat` section, which despawns it as it dies, and the armor of its starting armor set; the
/// tags of its kinds, of victory for a structure that counts for one, and of a structure, a wall
/// and infantry, which a shell's hits and a scatter name; and the turret of its AI module, which
/// turns its weapons. Every other object has none of these. A shell's type is a projectile alone
/// (12-zero-hour.md, The import converts shells, splash, scatter, pitch, turrets and
/// slow weapons).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ImportedType {
    #[serde(skip_serializing_if = "Option::is_none")]
    pools: Option<[&'static str; 1]>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<&'static str>,
    #[serde(skip_serializing_if = "TypeStats::is_empty")]
    stats: TypeStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    collision: Option<TypeBody>,
    #[serde(skip_serializing_if = "Option::is_none")]
    combat: Option<TypeCombat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    turret: Option<TypeTurret>,
    #[serde(skip_serializing_if = "Option::is_none")]
    projectile: Option<TypeProjectile>,
}

/// A type's stats: a driving type's `move_speed` in meters a second and its `turn_rate` in
/// degrees a second, as the original's units a second and degrees a second are, and a living
/// type's `life`, its body's `MaxHealth`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
struct TypeStats {
    #[serde(skip_serializing_if = "Option::is_none")]
    move_speed: Option<StatBase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    turn_rate: Option<StatBase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    life: Option<StatBase>,
}

/// A stat's value at every level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct StatBase {
    base: Num,
}

/// A type's body, as its `collision` section writes it: its shape and its height.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct TypeBody {
    #[serde(flatten)]
    shape: BodyShape,
    height: Num,
}

/// A body's shape: a circle's radius, or a box's sides, along its `x` then its `z`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
enum BodyShape {
    #[serde(rename = "radius")]
    Circle(Num),
    #[serde(rename = "box")]
    Box([Num; 2]),
}

/// A living type's `combat` section: it despawns as it dies, as the original's object goes, and
/// takes damage through its armor, whole with none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TypeCombat {
    on_death: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    armor: Option<DeclaredName>,
}

/// A type's `turret`: its turn rate in degrees a second, the original's 0.035 radians of
/// tolerance, and the slot kind of its weapons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct TypeTurret {
    turn_rate: Num,
    tolerance_deg: Num,
    slot: &'static str,
}

/// A shell's `projectile` section: its weapon's speed, twice its geometry's radius wide, what it
/// detonates on in its way, and its curve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct TypeProjectile {
    speed: Num,
    width: Num,
    hits: ShellHits,
    curve: TypeCurve,
}

/// What a shell detonates on in its way, besides its target: the units of any of its filters, or
/// none, `"none"`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ShellHits {
    Nothing,
    Units(Vec<&'static str>),
}

/// A shell's `curve`: its inner points, `[fraction, height]` each, how far its end follows its
/// victim a second, and its life in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct TypeCurve {
    first: [Num; 2],
    second: [Num; 2],
    follow: Num,
    life_ms: u64,
}

/// The mode's pool of life, and the stat of its maximum.
pub(crate) const LIFE: &str = "life";

/// The slot kind of the converted weapons.
pub(crate) const WEAPON: &str = "weapon";

/// The tag of a type that counts for a multiplayer victory (12-zero-hour.md, Defeat by the last
/// building that counts).
pub(crate) const VICTORY: &str = "counts_for_victory";

/// The tags of a structure, a wall and infantry, of the objects' `KindOf` and `FenceWidth`.
pub(crate) const STRUCTURE: &str = "structure";
pub(crate) const WALL: &str = "wall";
pub(crate) const INFANTRY: &str = "infantry";

/// The original's tolerance of a turret's aim, 0.035 radians (`TurretAIAimTurretState::update`),
/// in degrees, 2.00535228…, to the nearest bit: 33,644,228.41 · 2⁻²⁴.
const TURRET_TOLERANCE_DEG: Num = Num::from_bits(33_644_228);

/// The widest a driving type's circle is, 25 m: the original's largest pathfinding footprint, 2
/// cells of 10 m on each side of a center cell (`MAX_RADIUS` of `getRadiusAndCenter`), which our
/// one body for collision and pathfinding takes (12-zero-hour.md, Only walkers and standing
/// structures have bodies).
const MAX_WALKER: f32 = 25.0;

impl ImportedType {
    /// The unit type of `object`, its locomotors found in `locomotors`; an error for a locomotor
    /// no file defines, as the game refuses one, a number past what the engine holds, or a body
    /// a unit type's load refuses.
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
        let shape = match drive {
            Some(_) => Some(BodyShape::Circle(num(geometry
                .bounding_circle()
                .min(MAX_WALKER))?)),
            None if object.is_structure() && !geometry.small => Some(match geometry.shape {
                GeometryShape::Box => {
                    BodyShape::Box([num(2.0 * geometry.major)?, num(2.0 * geometry.minor)?])
                }
                GeometryShape::Sphere | GeometryShape::Cylinder => {
                    BodyShape::Circle(num(geometry.major)?)
                }
            }),
            None => None,
        };
        let collision = match shape {
            Some(shape) => Some(TypeBody {
                shape,
                height: num(geometry.height())?,
            }),
            None => None,
        };
        let held = collision.is_none_or(|body| {
            let shape = match body.shape {
                BodyShape::Circle(radius) => Radius::new(radius).is_some(),
                BodyShape::Box(size) => BodyBox::new(size, Num::ZERO).is_some(),
            };
            shape && Num::ZERO <= body.height && body.height <= MAX_BOUND
        });
        if !held {
            return Err(TypeError::Body);
        }
        let base = |value| Ok::<_, TypeError>(StatBase { base: num(value)? });
        let mut stats = TypeStats::default();
        if let Some(locomotor) = drive {
            stats.move_speed = Some(base(locomotor.speed)?);
            stats.turn_rate = Some(base(locomotor.turn_rate)?);
        }
        let fight = object.fight();
        let body = fight
            .body
            .filter(|body| matches!(body.kind, BodyKind::Active | BodyKind::Structure));
        stats.life = body
            .filter(|body| body.max > 0.0)
            .map(|body| base(body.max))
            .transpose()?;
        let combat = stats.life.map(|_| TypeCombat {
            on_death: "despawn",
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
        // A turret that turns no weapon, or does not turn, aims nothing the engine's turret aims.
        let turret = fight
            .turret
            .filter(|turret| turret.slots.contains(&true) && turret.turn_rate > 0.0)
            .map(|turret| {
                Ok::<_, TypeError>(TypeTurret {
                    turn_rate: num(turret.turn_rate)?,
                    tolerance_deg: TURRET_TOLERANCE_DEG,
                    slot: WEAPON,
                })
            })
            .transpose()?;
        Ok(Self {
            pools: stats.life.map(|_| [LIFE]),
            tags: kinds
                .into_iter()
                .filter_map(|(holds, tag)| holds.then_some(tag))
                .collect(),
            stats,
            collision,
            combat,
            turret,
            projectile: None,
        })
    }

    /// The type of a shell, of `projectile` alone: it has no life, no body and no tag.
    pub(crate) const fn shell(projectile: TypeProjectile) -> Self {
        Self {
            pools: None,
            tags: Vec::new(),
            stats: TypeStats {
                move_speed: None,
                turn_rate: None,
                life: None,
            },
            collision: None,
            combat: None,
            turret: None,
            projectile: Some(projectile),
        }
    }

    /// Its most life, of a type that lives.
    pub(crate) fn life(&self) -> Option<Num> {
        self.stats.life.map(|life| life.base)
    }

    /// Its armor, of a type that lives.
    pub(crate) fn armor(&self) -> Option<&DeclaredName> {
        self.combat.as_ref()?.armor.as_ref()
    }

    /// How far its body reaches from its unit's place along `x` and `z`, at `angle` degrees, as
    /// a map's load measures a placed box against its bounds; none for no body.
    pub(crate) fn reach(&self, angle: Num) -> Option<[Num; 2]> {
        match self.collision?.shape {
            BodyShape::Circle(radius) => Some([radius; 2]),
            BodyShape::Box(size) => Some(
                BodyBox::new(size, angle)
                    .expect("a box's size alone decides whether it is one")
                    .extent(),
            ),
        }
    }

    /// Whether its units walk: it drives, so it has a `move_speed`.
    pub(crate) const fn walks(&self) -> bool {
        self.stats.move_speed.is_some()
    }

    /// Whether its body is a box, which a build places.
    pub(crate) const fn is_box(&self) -> bool {
        matches!(
            self.collision,
            Some(TypeBody {
                shape: BodyShape::Box(_),
                ..
            })
        )
    }

    /// Its speed in meters a second, when it drives.
    pub(crate) const fn move_speed(&self) -> Option<Num> {
        match self.stats.move_speed {
            Some(speed) => Some(speed.base),
            None => None,
        }
    }
}

impl TypeProjectile {
    /// The projectile `weapon` fires, of `shell`, an object of a `DumbProjectileBehavior`: at its
    /// `WeaponSpeed`, twice its geometry's radius wide, detonating on what its
    /// `ProjectileCollidesWith` names, on the curve of its behavior; why the import drops the
    /// weapon, for a speed that is not above 0, a mask of what the engine's units are not, or a
    /// number past what the engine holds.
    pub(crate) fn of(weapon: &IniWeapon, shell: &IniObject) -> Result<Self, WeaponDrop> {
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
        let filters: Vec<&'static str> = filters
            .into_iter()
            .filter_map(|(holds, filter)| holds.then_some(filter))
            .collect();
        let number = |value: f32| game_float::to_num(value).map_err(|_| WeaponDrop::BadNumber);
        let speed = number(weapon.speed)?;
        let width = number(2.0 * shell.geometry().major)?;
        let fraction = |value: f32| {
            let value = number(value)?;
            (Num::ZERO..=Num::ONE)
                .contains(&value)
                .then_some(value)
                .ok_or(WeaponDrop::BadNumber)
        };
        let height = |value: f32| {
            let value = number(value)?;
            (-MAX_BOUND..=MAX_BOUND)
                .contains(&value)
                .then_some(value)
                .ok_or(WeaponDrop::BadNumber)
        };
        let follow = number(flight.follow)?;
        let life_ms = weapon_drop::game_ms(flight.lifespan_ms)?;
        let held = speed > Num::ZERO
            && Num::ZERO <= width
            && width <= MAX_BOUND * 2
            && follow >= Num::ZERO
            && life_ms > 0;
        if !held {
            return Err(WeaponDrop::BadNumber);
        }
        Ok(Self {
            speed,
            width,
            hits: if filters.is_empty() {
                ShellHits::Nothing
            } else {
                ShellHits::Units(filters)
            },
            curve: TypeCurve {
                first: [fraction(flight.first_indent)?, height(flight.first_height)?],
                second: [
                    fraction(flight.second_indent)?,
                    height(flight.second_height)?,
                ],
                follow,
                life_ms,
            },
        })
    }
}

/// As data writes it: `"none"`, or the list of its filters.
impl Serialize for ShellHits {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Nothing => serializer.serialize_str("none"),
            Self::Units(filters) => filters.serialize(serializer),
        }
    }
}

impl TypeStats {
    /// Whether it holds no stat, so the type writes no `stats`.
    const fn is_empty(&self) -> bool {
        self.move_speed.is_none() && self.turn_rate.is_none() && self.life.is_none()
    }
}

/// `value` exactly as a `Num`; an error for one that is not finite, or past what a `Num` holds.
fn num(value: f32) -> Result<Num, TypeError> {
    game_float::to_num(value).map_err(|_| TypeError::Number)
}

#[cfg(test)]
pub(crate) mod internals {
    use campfire_math::Num;

    use crate::zero_hour::imported_type::{ShellHits, TypeCurve, TypeProjectile};

    /// The projectile of the fixture's `TankShell`, fired at `speed` m/s by a weapon of
    /// `STRUCTURES WALLS`: 2 m wide, its inner points at 50% and 90% of the way, 10 m up, its end
    /// following its victim by 100 m/s, for 10 s. 90% is 90 ÷ 100 in `f32`, 0.89999997615…, the
    /// bits 15,099,494 · 2⁻²⁴ of a `Num`.
    pub(crate) fn shell_projectile(speed: i64) -> TypeProjectile {
        TypeProjectile {
            speed: Num::int(speed),
            width: Num::int(2),
            hits: ShellHits::Units(vec!["enemies:structure", "all:wall"]),
            curve: TypeCurve {
                first: [Num::HALF, Num::int(10)],
                second: [Num::from_bits(15_099_494), Num::int(10)],
                follow: Num::int(100),
                life_ms: 10_000,
            },
        }
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
        let body = |shape, height| Some(TypeBody { shape, height });
        let drives = |speed, turn_rate, radius, height| ImportedType {
            pools: None,
            tags: Vec::new(),
            stats: TypeStats {
                move_speed: Some(StatBase { base: speed }),
                turn_rate: Some(StatBase { base: turn_rate }),
                life: None,
            },
            collision: body(BodyShape::Circle(radius), height),
            combat: None,
            turret: None,
            projectile: None,
        };
        let stands = |body| ImportedType {
            pools: None,
            tags: Vec::new(),
            stats: TypeStats::default(),
            collision: body,
            combat: None,
            turret: None,
            projectile: None,
        };
        // A structure stands of the tag of its kind.
        let structure = |body| ImportedType {
            tags: vec![STRUCTURE],
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
            structure(body(BodyShape::Box([int(120), int(140)]), int(49)))
        );
        assert_eq!(
            of("Tower").unwrap(),
            structure(body(BodyShape::Circle(half(17)), int(1)))
        );
        assert_eq!(
            of("Shrub").unwrap(),
            structure(body(BodyShape::Circle(int(1)), int(1)))
        );
        assert_eq!(
            of("Dome").unwrap(),
            structure(body(BodyShape::Circle(int(5)), int(5)))
        );
        // A small structure and an object of no kind block nothing; a reskin's own geometry
        // makes its mine large, a box of 6 × 6.
        assert_eq!(of("Mine").unwrap(), structure(None));
        assert_eq!(of("Rock").unwrap(), stands(None));
        assert_eq!(
            of("BigMine").unwrap(),
            structure(body(BodyShape::Box([int(6), int(6)]), int(1)))
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
        let lives = |life: i64, armor: Option<&str>, tags| ImportedType {
            pools: Some([LIFE]),
            tags,
            stats: TypeStats {
                life: Some(StatBase {
                    base: Num::int(life),
                }),
                ..TypeStats::default()
            },
            collision: None,
            combat: Some(TypeCombat {
                on_death: "despawn",
                armor: armor.map(|armor| DeclaredName::new(armor).unwrap()),
            }),
            turret: None,
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
        let center = ImportedType {
            collision: Some(TypeBody {
                shape: BodyShape::Circle(Num::ONE),
                height: Num::ONE,
            }),
            ..lives(5000, None, vec![VICTORY, STRUCTURE])
        };
        assert_eq!(of("Center"), center);
        assert_eq!(of("Wall"), lives(10, None, vec![WALL]));
        // An immortal body, and a body of no most life, give no life, and no combat.
        for name in ["Ghost", "Hollow"] {
            let typed = of(name);
            assert_eq!((typed.pools, typed.combat), (None, None), "{name}");
        }
        // The gunner's turret turns 60° a second, its weapons within the original's 0.035
        // radians; a turret of no weapon, or of no turn, is none.
        let turret = TypeTurret {
            turn_rate: Num::int(60),
            tolerance_deg: TURRET_TOLERANCE_DEG,
            slot: WEAPON,
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
            TypeProjectile::of(
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
        let hits = |filters: &[&'static str]| Ok(ShellHits::Units(filters.to_vec()));
        assert_eq!(
            fired("").map(|shell| shell.hits),
            hits(&["enemies:structure"])
        );
        assert_eq!(
            collides("ALLIES ENEMIES CONTROLLED_STRUCTURES"),
            hits(&["allies", "hostiles", "allies:structure"])
        );
        assert_eq!(collides("NONE"), Ok(ShellHits::Nothing));
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
