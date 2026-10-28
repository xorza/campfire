use std::collections::BTreeMap;
use std::num::NonZeroU32;

use campfire_actions::{ActionKeys, ActionRange, RangeField, Targeting};
use campfire_codec::TomlTable;
use campfire_common::DeclaredName;
use campfire_interaction::{
    EnterActionData, EnterData, FireData, PayloadData, RiderCount, RidesData,
};
use campfire_math::Num;
use campfire_values::{
    FilterData, FilterTag, Number, Ranked, RelationSet, Scalar, Share, Stat, TagFilterData,
};
use serde::Serialize;

use crate::zero_hour::axes;
use crate::zero_hour::contain_drop_cause::ContainDropCause;
use crate::zero_hour::dropped::Losses;
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::error::FloatError;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::game_data_ini::GarrisonBonus;
use crate::zero_hour::format::ini::object_ini::IniObject;
use crate::zero_hour::format::ini::object_ini::ini_contain::{ContainClass, IniContain};
use crate::zero_hour::format::ini::object_ini::object_kinds::{ObjectKind, ObjectKinds};
use crate::zero_hour::game_time;
use crate::zero_hour::import_names::TypedObject;
use crate::zero_hour::portable_structure;
use crate::zero_hour::rules_names::RulesNames;
use crate::zero_hour::supply_import::DOCK_RANGE;
use crate::zero_hour::type_name::TypeName;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// Zero Hour's transports and garrisons, of the unit types the import makes
/// (04-capabilities/interaction.md, A container says who rides; 04-capabilities/combat.md, A
/// container may let its riders fire; 04-capabilities/interaction.md, A garrison's riders see from
/// it; 04-capabilities/combat.md, A container's modifier holds on its riders): the `enter` of each
/// type of a `TransportContain` or a `GarrisonContain`, the `rides` of each type that walks and
/// takes room of a transport, the entry they name, the garrison's modifier of `GameData.ini`'s
/// `GARRISONED` bonuses, and what it drops, each once for each cause, in the order of the types'
/// names.
#[derive(Debug, Default)]
pub(crate) struct ContainImport {
    pub(crate) enters: BTreeMap<TypeName, EnterData>,
    pub(crate) entries: BTreeMap<DeclaredName, ActionKeys>,
    pub(crate) rides: BTreeMap<TypeName, RidesData>,
    pub(crate) modifiers: BTreeMap<DeclaredName, GarrisonModifier>,
    pub(crate) dropped: Losses<ContainDropCause>,
}

/// The garrison's modifier as the mode's data writes it: what it adds to each stat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct GarrisonModifier {
    stats: BTreeMap<Stat, Number>,
}

/// A container's riders, of its object's `contain`, as `of` converts them: the import's riders,
/// by their types' names, and each one's kinds, which a container's filters are tested on.
#[derive(Debug, Clone, Copy)]
struct RiderTypes<'a> {
    rides: &'a BTreeMap<TypeName, RidesData>,
    kinds: &'a [ObjectKinds],
}

impl ContainImport {
    /// The containers and the riders of `types`, by the names, the thresholds and the bonuses of
    /// `rules`, a garrison's fire points of `fire_points`, by its type's name; an error for a
    /// garrison's bonus that is no number the engine holds.
    pub(crate) fn of<'n, 'r: 'n>(
        rules: &ZeroHourRules,
        types: impl Iterator<Item = TypedObject<'n, 'r>>,
        fire_points: &BTreeMap<DeclaredName, Vec<[f32; 2]>>,
    ) -> Result<Self, ZeroHourError> {
        let names = &rules.names;
        let types: Vec<TypedObject<'n, 'r>> = types.collect();
        let mut import = Self::default();
        let mut kinds = Vec::new();
        for typed in &types {
            let room = u32::try_from(typed.object.transport_slots()).ok();
            let Some(size) = room.and_then(NonZeroU32::new) else {
                continue;
            };
            if typed.imported.is_projectile() || !typed.imported.walks() {
                continue;
            }
            import.rides.insert(typed.name.clone(), RidesData { size });
            kinds.push(typed.object.kinds());
        }
        let mut by_object = BTreeMap::new();
        for typed in &types {
            by_object.insert(typed.object.name.as_str(), typed.name);
        }
        let riders = RiderTypes {
            rides: &import.rides,
            kinds: &kinds,
        };
        let mut enters = BTreeMap::new();
        let mut dropped = Losses::default();
        for typed in &types {
            let object = typed.object;
            let Some(contain) = object.contain() else {
                continue;
            };
            let points = fire_points.get(typed.name.declared());
            let mut drop = |cause| dropped.drop(&object.name, cause);
            // An Overlord's riders are its battle bunker's, which its upgrade makes inside it,
            // open once it holds the upgrade's tag (04-capabilities/interaction.md, A bunker is an
            // entry a tag opens).
            let bunker = (contain.class == ContainClass::Overlord)
                .then(|| {
                    object.upgrades().iter().find_map(|module| {
                        let carried = portable_structure::transport(rules, module)?;
                        let contain = carried.contain()?;
                        (!module.triggers.is_empty()).then_some((carried, contain, module))
                    })
                })
                .flatten();
            let (entered, contain, open) = match bunker {
                Some((carried, carried_contain, module)) => {
                    let tag = portable_structure::tag(module)?.expect("a module of a trigger");
                    let opened = FilterTag {
                        name: tag,
                        negated: false,
                    };
                    (carried, carried_contain, Some(TagFilterData::new([opened])))
                }
                None => (object, contain, None),
            };
            let mut enter = match Self::enter(entered, contain, rules, riders, points, &mut drop) {
                Ok(enter) => EnterData { open, ..enter },
                Err(cause) => {
                    drop(cause);
                    continue;
                }
            };
            // A payload of no rider makes none, as `createPayload` counts up to it.
            let payload = contain.payload.as_ref();
            if let Some(payload) = payload.filter(|payload| payload.count > 0) {
                let fits = by_object
                    .get(payload.name.as_str())
                    .and_then(|&rider| Self::payload(&enter, rider, payload.count, riders));
                match fits {
                    Some(payload) => enter.payload = vec![payload],
                    None => drop(ContainDropCause::Payload),
                }
            }
            enters.insert(typed.name.clone(), enter);
        }
        if !enters.is_empty() {
            import.entries.insert(names.enter.clone(), Self::entry());
        }
        import.enters = enters;
        import.dropped = dropped;
        let modifier = GarrisonModifier::of(rules.game_data.garrison, names)
            .map_err(|_| ZeroHourError::GarrisonBonus)?;
        import.modifiers.insert(names.garrisoned.clone(), modifier);
        Ok(import)
    }

    /// The `enter` of `object`'s `contain`, its payload left out, by the names and the threshold
    /// of `rules`, which takes those of `riders` its kinds take, by tags that take each as its
    /// masks do (09-zero-hour.md, The import says who rides by tags, or drops the container), and
    /// fires from `points`, or from its place of none or of no enclosing (09-zero-hour.md, A
    /// garrison that encloses no rider fires from its place), each part it drops given to `drop`;
    /// the cause as an error for a contain module it does not convert.
    fn enter(
        object: &IniObject,
        contain: &IniContain,
        rules: &ZeroHourRules,
        riders: RiderTypes<'_>,
        points: Option<&Vec<[f32; 2]>>,
        drop: &mut impl FnMut(ContainDropCause),
    ) -> Result<EnterData, ContainDropCause> {
        let names = &rules.names;
        if let Some(cause) = Self::lost(contain.class) {
            return Err(cause);
        }
        let garrison = contain.class == ContainClass::Garrison;
        let Some(capacity) = u32::try_from(contain.capacity)
            .ok()
            .filter(|&room| room > 0)
        else {
            return Err(ContainDropCause::NoRoom);
        };
        let relations = match (garrison, contain.allies, contain.enemies, contain.neutral) {
            // A transport takes its own player's units alone, which are its allies.
            (false, true, _, _) | (true, true, true, true) => RelationSet::All,
            (true, true, false, false) => RelationSet::Allies,
            (true, false, false, true) => RelationSet::Neutrals,
            _ => return Err(ContainDropCause::Relations),
        };
        let mut forbid = contain.forbid;
        if garrison {
            forbid.kinds = forbid.kinds.with(ObjectKind::NoGarrison);
        }
        if contain.allow.unread || forbid.unread {
            return Err(ContainDropCause::Kinds);
        }
        let tags = names.kind_tags();
        let tagged = ObjectKinds::of(&tags.map(|(kind, _)| kind));
        let [allow, forbid] = [contain.allow.kinds, forbid.kinds];
        let takes = |kinds: ObjectKinds, allow: ObjectKinds, forbid: ObjectKinds| {
            kinds.any_of(allow) && !kinds.any_of(forbid)
        };
        let [allow_tagged, forbid_tagged] = [allow, forbid].map(|mask| mask.intersection(tagged));
        let exact = riders
            .kinds
            .iter()
            .all(|&kinds| takes(kinds, allow, forbid) == takes(kinds, allow_tagged, forbid_tagged));
        if !exact {
            return Err(ContainDropCause::Kinds);
        }
        if !riders
            .kinds
            .iter()
            .any(|&kinds| takes(kinds, allow, forbid))
        {
            return Err(ContainDropCause::NoRider);
        }
        let refused = tags.iter().filter(|(kind, _)| forbid.holds(*kind));
        let refused: Vec<FilterTag> = refused
            .map(|(_, tag)| FilterTag {
                name: (*tag).clone(),
                negated: true,
            })
            .collect();
        let filters = tags
            .iter()
            .filter(|(kind, _)| allow.holds(*kind))
            .map(|(_, tag)| {
                let taken = FilterTag {
                    name: (*tag).clone(),
                    negated: false,
                };
                FilterData {
                    relations,
                    tags: [taken].into_iter().chain(refused.iter().cloned()).collect(),
                }
            });
        let share = |value: f32| Self::share(value).ok_or(ContainDropCause::BadNumber);
        let damage = share(contain.damage)?;
        let lasts = object.kinds().holds(ObjectKind::GarrisonableUntilDestroyed);
        let full = (garrison && !lasts)
            .then(|| share(rules.game_data.really_damaged_threshold))
            .transpose()?
            .unwrap_or(Share::NONE);
        let exit_ms = game_time::of_duration(contain.exit_ms as f32)
            .ok()
            .and_then(|ms| u32::try_from(ms).ok())
            .ok_or(ContainDropCause::BadNumber)?;
        if contain.heal > 0.0 {
            drop(ContainDropCause::RiderHeal);
        }
        let every = || TagFilterData::new([]);
        let fire = match (garrison, contain.enclosing) {
            (false, _) if contain.fire => {
                let infantry = FilterTag {
                    name: names.infantry.clone(),
                    negated: false,
                };
                FireData::Center(TagFilterData::new([infantry]))
            }
            (false, _) => FireData::None,
            (true, true) => match points.map(|points| Self::points(points)) {
                Some(Some(points)) => FireData::Points(points),
                Some(None) => return Err(ContainDropCause::BadNumber),
                None => FireData::Center(every()),
            },
            (true, false) => {
                drop(ContainDropCause::StationPoints);
                FireData::Center(every())
            }
        };
        Ok(EnterData {
            action: names.enter.clone(),
            capacity,
            counts: if garrison {
                RiderCount::One
            } else {
                RiderCount::Size
            },
            riders: filters.collect(),
            exit_ms,
            damage,
            full,
            capture: garrison && !object.kinds().faction(),
            payload: Vec::new(),
            fire,
            sight: garrison,
            modifier: garrison.then(|| names.garrisoned.clone()),
            open: None,
            assault: object
                .fight()
                .assault
                .map(|ratio| Self::share(ratio).ok_or(ContainDropCause::BadNumber))
                .transpose()?,
        })
    }

    /// The entry of every container, which a rider runs from within the pathfinder's cell of its
    /// body (09-zero-hour.md, The import's entry reaches the pathfinder's cell).
    fn entry() -> ActionKeys {
        let range = RangeField::Range(ActionRange::meters(DOCK_RANGE));
        ActionKeys {
            range: Some(Ranked::One(range)),
            ..ActionKeys::new(Targeting::Unit(FilterData::all()))
        }
    }

    /// Its entries' tables under `[actions]`.
    pub(crate) fn entry_tables(&self) -> impl Iterator<Item = (&DeclaredName, TomlTable)> {
        let section = EnterActionData::default();
        self.entries
            .iter()
            .map(move |(id, keys)| (id, keys.table(&section)))
    }

    /// The payload of `count` of the type `rider`, above 0, in a container of `enter`; none past
    /// its room or of a type that does not ride.
    fn payload(
        enter: &EnterData,
        rider: &TypeName,
        count: i32,
        riders: RiderTypes<'_>,
    ) -> Option<PayloadData> {
        let count = u32::try_from(count).ok()?;
        let size = riders.rides.get(rider)?.size.get();
        let each = match enter.counts {
            RiderCount::Size => size,
            RiderCount::One => 1,
        };
        let room = u64::from(each) * u64::from(count);
        (room <= u64::from(enter.capacity)).then(|| PayloadData {
            unit: rider.declared().clone(),
            count,
        })
    }

    /// The share `value` of the game's `f32` holds, to the nearest billionth a share holds; none
    /// below 0 or past 1.
    fn share(value: f32) -> Option<Share> {
        if !(0.0..=1.0).contains(&value) {
            return None;
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a share from 0 to 1 is from 0 to a billion billionths"
        )]
        let billionths = (f64::from(value) * 1e9).round() as u32;
        Share::billionths(billionths)
    }

    /// The fire points `points`, the game's axes, on the engine's; none past what it holds.
    fn points(points: &[[f32; 2]]) -> Option<Vec<[Num; 2]>> {
        points
            .iter()
            .map(|&[x, y]| {
                let [x, y] = [x, y].map(|axis| game_float::to_num(axis).ok());
                axes::ground([x?, y?])
            })
            .collect()
    }

    /// Why a contain module of `class` is dropped; none for a transport's or a garrison's.
    const fn lost(class: ContainClass) -> Option<ContainDropCause> {
        Some(match class {
            ContainClass::Transport | ContainClass::Garrison | ContainClass::Helix => return None,
            ContainClass::Open => ContainDropCause::Open,
            ContainClass::Cave => ContainDropCause::Cave,
            ContainClass::Heal => ContainDropCause::Heal,
            ContainClass::InternetHack => ContainDropCause::InternetHack,
            ContainClass::RailedTransport => ContainDropCause::RailedTransport,
            ContainClass::RiderChange => ContainDropCause::RiderChange,
            ContainClass::MobNexus => ContainDropCause::MobNexus,
            ContainClass::Tunnel => ContainDropCause::Tunnel,
            ContainClass::Overlord => ContainDropCause::Overlord,
            ContainClass::Parachute => ContainDropCause::Parachute,
        })
    }
}

impl GarrisonModifier {
    /// The modifier of `bonus`: what it adds to the range and the damage of every weapon of a
    /// rider, its bonus less 1, as `WeaponBonusSet::appendBonuses` adds it, each in the game's
    /// `f32`, and none of a bonus of 1, in `names`' stats.
    fn of(bonus: GarrisonBonus, names: &RulesNames) -> Result<Self, FloatError> {
        let stats = [
            (&names.garrison_range, bonus.range),
            (&names.garrison_damage, bonus.damage),
        ];
        let mut added = BTreeMap::new();
        for (stat, factor) in stats {
            let value = game_float::to_num(factor - 1.0)?;
            if value != Num::ZERO {
                added.insert(stat.clone(), Number::Value(Scalar::Decimal(value)));
            }
        }
        Ok(Self { stats: added })
    }
}

#[cfg(test)]
mod tests;
