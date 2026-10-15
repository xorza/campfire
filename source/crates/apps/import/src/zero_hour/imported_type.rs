use campfire_geometry::{BodyBox, Radius};
use campfire_math::Num;
use campfire_values::DeclaredName;
use serde::Serialize;

use crate::zero_hour::error::TypeError;
use crate::zero_hour::locomotor_ini::LocomotorIni;
use crate::zero_hour::object_ini::fight_data::BodyKind;
use crate::zero_hour::object_ini::{GeometryShape, IniObject};

/// An object's unit type as the import writes it in `data/units.toml` (design 17, D1, D4; design
/// 19, D5, D7): the speed and the turn rate of its first normal locomotor that drives on the
/// ground; its body: a circle of its bounding radius, at most `MAX_WALKER`, when it drives, or,
/// when it is a structure that is not small, a box of twice its radii for a box geometry, else a
/// circle of its radius; the life of an active or a structure body, with its `combat` section,
/// which despawns it as it dies, and the armor of its starting armor set; and the tag of victory
/// for a structure that counts for one. Every other object has none of these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ImportedType {
    #[serde(skip_serializing_if = "Option::is_none")]
    pools: Option<[&'static str; 1]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<[&'static str; 1]>,
    #[serde(skip_serializing_if = "TypeStats::is_empty")]
    stats: TypeStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    collision: Option<TypeBody>,
    #[serde(skip_serializing_if = "Option::is_none")]
    combat: Option<TypeCombat>,
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

/// A type's body, as its `collision` section writes it: a circle's radius, or a box's sides,
/// along its `x` then its `z`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
enum TypeBody {
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

/// The mode's pool of life, and the stat of its maximum.
pub(crate) const LIFE: &str = "life";

/// The tag of a type that counts for a multiplayer victory (design 19, D7).
pub(crate) const VICTORY: &str = "counts_for_victory";

/// The widest a driving type's circle is, 25 m: the original's largest pathfinding footprint, 2
/// cells of 10 m on each side of a center cell (`MAX_RADIUS` of `getRadiusAndCenter`), which our
/// one body for collision and pathfinding takes (design 17, D4).
const MAX_WALKER: f32 = 25.0;

impl ImportedType {
    /// The unit type of `object`, its locomotors found in `locomotors`; an error for a locomotor
    /// no file defines, as the game refuses one, a number past what the engine holds, or a body
    /// a unit type's load refuses.
    pub(crate) fn of(
        object: &IniObject,
        locomotors: &LocomotorIni,
    ) -> Result<ImportedType, TypeError> {
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
        let collision = match drive {
            Some(_) => Some(TypeBody::Circle(num(geometry
                .bounding_circle()
                .min(MAX_WALKER))?)),
            None if object.is_structure() && !geometry.small => Some(match geometry.shape {
                GeometryShape::Box => {
                    TypeBody::Box([num(2.0 * geometry.major)?, num(2.0 * geometry.minor)?])
                }
                GeometryShape::Sphere | GeometryShape::Cylinder => {
                    TypeBody::Circle(num(geometry.major)?)
                }
            }),
            None => None,
        };
        let held = match collision {
            Some(TypeBody::Circle(radius)) => Radius::new(radius).is_some(),
            Some(TypeBody::Box(size)) => BodyBox::new(size, Num::ZERO).is_some(),
            None => true,
        };
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
        let combat = match stats.life {
            Some(_) => {
                let armor = fight.starting_armor().map(|armor| {
                    DeclaredName::new(&armor.to_ascii_lowercase())
                        .ok_or_else(|| TypeError::ArmorName(armor.to_owned()))
                });
                Some(TypeCombat {
                    on_death: "despawn",
                    armor: armor.transpose()?,
                })
            }
            None => None,
        };
        Ok(ImportedType {
            pools: stats.life.map(|_| [LIFE]),
            tags: object.counts_for_victory().then_some([VICTORY]),
            stats,
            collision,
            combat,
        })
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
        match self.collision? {
            TypeBody::Circle(radius) => Some([radius; 2]),
            TypeBody::Box(size) => Some(
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
        matches!(self.collision, Some(TypeBody::Box(_)))
    }

    /// Its speed in meters a second, when it drives.
    pub(crate) const fn move_speed(&self) -> Option<Num> {
        match self.stats.move_speed {
            Some(speed) => Some(speed.base),
            None => None,
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
    Num::try_from(value).map_err(|_| TypeError::Number)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::locomotor_ini::internals::LOCOMOTORS;
    use crate::zero_hour::object_ini::ObjectIni;

    /// The objects of each kind D4 tells apart.
    const OBJECTS: &str = "Object Tank
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
";

    #[test]
    fn an_objects_type_drives_by_its_ground_locomotor_and_has_its_body_by_d4() {
        let mut ini = ObjectIni::default();
        ini.read(OBJECTS.as_bytes()).unwrap();
        let mut locomotors = LocomotorIni::default();
        locomotors.read(LOCOMOTORS.as_bytes()).unwrap();
        let of = |name: &str| ImportedType::of(ini.object(name.as_bytes()).unwrap(), &locomotors);
        let int = Num::int;
        let half = |x: i64| Num::from_bits(x << (Num::FRAC_BITS - 1));
        let drives = |speed, turn_rate, radius| ImportedType {
            pools: None,
            tags: None,
            stats: TypeStats {
                move_speed: Some(StatBase { base: speed }),
                turn_rate: Some(StatBase { base: turn_rate }),
                life: None,
            },
            collision: Some(TypeBody::Circle(radius)),
            combat: None,
        };
        let stands = |body| ImportedType {
            pools: None,
            tags: None,
            stats: TypeStats::default(),
            collision: body,
            combat: None,
        };
        // The tank drives on its treads, 30.5 m and 90° a second; a box of half extents 15 and
        // 10 bounds a circle of √(15² + 10²) = √325 = 18.02775637…, in `f32` the nearest of
        // 19 bits past the point, 9,451,736 · 2⁻¹⁹ = 18.0277557373…, exactly 302,455,552 · 2⁻²⁴
        // as a `Num`.
        let tank = of("Tank").unwrap();
        assert_eq!(tank, drives(half(61), int(90), Num::from_bits(302_455_552)));
        assert_eq!(tank.move_speed(), Some(half(61)));
        // The jet only flies: no stats, and no body. The hover craft's first normal locomotor
        // flies, and its second drives, a cylinder's major radius 6.5.
        assert_eq!(of("Jet").unwrap(), stands(None));
        assert_eq!(of("Jet").unwrap().move_speed(), None);
        assert_eq!(of("Hover").unwrap(), drives(half(61), int(90), half(13)));
        // A train's box bounds a circle of √(112² + 10²) ≈ 112.4 m, past the original's widest
        // pathfinding footprint: its circle is that footprint's 25 m.
        assert_eq!(of("Train").unwrap(), drives(half(61), int(90), int(25)));
        // A structure that is not small: a box of twice its radii, 120 × 140; a cylinder's
        // circle of its radius 8.5; one of no geometry the game's sphere of radius 1.
        assert_eq!(
            of("Barracks").unwrap(),
            stands(Some(TypeBody::Box([int(120), int(140)])))
        );
        assert_eq!(
            of("Tower").unwrap(),
            stands(Some(TypeBody::Circle(half(17))))
        );
        assert_eq!(of("Shrub").unwrap(), stands(Some(TypeBody::Circle(int(1)))));
        // A small structure and an object of no kind block nothing; a reskin's own geometry
        // makes its mine large, a box of 6 × 6.
        assert_eq!(of("Mine").unwrap(), stands(None));
        assert_eq!(of("Rock").unwrap(), stands(None));
        assert_eq!(
            of("BigMine").unwrap(),
            stands(Some(TypeBody::Box([int(6), int(6)])))
        );
        // A locomotor no file defines refuses the type, as the game refuses it.
        assert_eq!(
            of("Lost"),
            Err(TypeError::NoLocomotor("NoSuchTreads".to_owned()))
        );
        // A box of no width, and a circle past the widest body's 2,048 m, a unit type's load
        // refuses, so the import refuses them first.
        assert_eq!(of("Flat"), Err(TypeError::Body));
        assert_eq!(of("Huge"), Err(TypeError::Body));
    }

    /// The objects of each kind of life D5 and D7 tell apart.
    const LIVING: &str = "Object Soldier
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
  Body = ActiveBody ModuleTag_03
    MaxHealth = 10
  End
  ArmorSet
    Conditions = None
    Armor = None
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
    fn a_living_object_lives_by_its_body_takes_its_starting_armor_and_may_count_for_victory() {
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
        };
        // The soldier lives by its active body, 180, in the armor of its set of no condition,
        // the second it gives, by the name the mode declares; a reskin keeps its base's body,
        // as its table reads none.
        assert_eq!(of("Soldier"), lives(180, Some("humanarmor"), None));
        assert_eq!(of("Veteran"), of("Soldier"));
        // A structure that counts for victory lives by its structure body, of no armor, and of
        // no geometry stands as the game's sphere of radius 1; an
        // object of the kind that is no structure, of an armor of `None`, counts for none.
        let center = ImportedType {
            collision: Some(TypeBody::Circle(Num::ONE)),
            ..lives(5000, None, Some([VICTORY]))
        };
        assert_eq!(of("Center"), center);
        assert_eq!(of("Wall"), lives(10, None, None));
        // An immortal body, and a body of no most life, give no life, and no combat.
        for name in ["Ghost", "Hollow"] {
            let typed = of(name);
            assert_eq!((typed.pools, typed.combat), (None, None), "{name}");
        }
    }
}
