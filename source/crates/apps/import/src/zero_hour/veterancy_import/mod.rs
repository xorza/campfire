use std::collections::{BTreeMap, BTreeSet};

use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_progression::{
    ExperienceData, ExperienceTo, Gives, ShareData, StartData, Thresholds, TrackData,
    TypeTrackData, TypeTracksData,
};
use campfire_stats::Level;
use campfire_values::{Number, Scalar, Stat};
use serde::{Serialize, Serializer};

use crate::zero_hour::error::{NameError, ZeroHourError};
use crate::zero_hour::format::error::FloatError;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::game_data_ini::{GameDataIni, VeterancyBonus};
use crate::zero_hour::format::ini::object_ini::ini_experience::IniExperience;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::import_names::TypedObject;
use crate::zero_hour::rules_names::RulesNames;
use crate::zero_hour::type_name::TypeName;

/// The names of the modifiers of a veteran, an elite and a heroic unit, levels 2 to 4 of the
/// track, as `TheVeterancyNames` names the levels.
const LEVEL_MODIFIERS: [&str; 3] = ["veteran", "elite", "heroic"];

/// The import's veterancy (09-zero-hour.md, Veterancy is progression's data): the mode's `level`
/// track `veterancy`, of the commonest thresholds of the trainable types, each level from 2 holding
/// its modifier and none giving a point, and its share, to a hostile killer that has the track, of
/// every death but a site's, as `Object::scoreTheKill` gives none to an ally or the neutral player
/// and none for a victim `UNDER_CONSTRUCTION`; the modifiers of its levels, of `GameData.ini`'s
/// bonuses; each trainable type's entry on it, of its own thresholds where they are not the track's
/// and the levels its `VeterancyGainCreate` modules start it at, each always or by its science's
/// perk's modifier (04-capabilities/progression.md, A start level by a modifier); each type's
/// `experience`, its `ExperienceValue` by level; and the trainable objects of thresholds that do
/// not ascend from 0, which gain no level, by name.
#[derive(Debug, Default)]
pub(crate) struct VeterancyImport {
    pub(crate) track: Option<(DeclaredName, TrackData)>,
    pub(crate) share: Option<ShareData>,
    pub(crate) modifiers: BTreeMap<DeclaredName, LevelModifier>,
    pub(crate) tracks: BTreeMap<TypeName, TypeTracksData>,
    pub(crate) experience: BTreeMap<TypeName, ExperienceData>,
    pub(crate) untrainable: Vec<String>,
    /// The sciences the starts need, by their names in the game, whose perks give their
    /// modifiers.
    pub(crate) sciences: BTreeSet<String>,
}

/// A level's modifier as the mode's data writes it: what it adds to each stat, or the share it
/// raises one by.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct LevelModifier {
    stats: BTreeMap<Stat, Change>,
}

/// A stat's change: an amount added, or a percent, `{ pct }`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Change {
    Add(Number),
    Pct(Number),
}

impl Serialize for Change {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Debug, Serialize)]
        struct Pct<'a> {
            pct: &'a Number,
        }
        match self {
            Self::Add(value) => value.serialize(serializer),
            Self::Pct(pct) => Pct { pct }.serialize(serializer),
        }
    }
}

impl VeterancyImport {
    /// The veterancy of `types`, of `game_data`'s bonuses, its stats and its life `names`'.
    /// An error for a bonus of `game_data` that is no number the engine holds.
    pub(crate) fn of<'n, 'r: 'n>(
        game_data: &GameDataIni,
        types: impl Iterator<Item = TypedObject<'n, 'r>>,
        names: &RulesNames,
    ) -> Result<Self, ZeroHourError> {
        let mut import = Self::default();
        let mut trained = BTreeMap::<TypeName, (Thresholds, Vec<StartData>)>::new();
        for TypedObject {
            name,
            object,
            imported,
        } in types
        {
            if imported.is_projectile() || imported.life().is_none() {
                continue;
            }
            let experience = object.experience();
            if experience.values.iter().any(|&value| value > 0) {
                let values = experience
                    .values
                    .map(|value| Num::int(i64::from(value.max(0))));
                let gives = Gives::ByLevel(values.to_vec());
                import
                    .experience
                    .insert(name.clone(), ExperienceData { gives });
            }
            if !experience.trainable {
                continue;
            }
            let Some(thresholds) = Self::thresholds(experience) else {
                import.untrainable.push(object.name.clone());
                continue;
            };
            let starts = Self::starts(experience)?;
            let sciences = experience
                .starts
                .iter()
                .filter_map(|start| start.science.clone());
            import.sciences.extend(sciences);
            trained.insert(name.clone(), (thresholds, starts));
        }
        let Some(common) = Self::commonest(trained.values().map(|(thresholds, _)| thresholds))
        else {
            return Ok(import);
        };
        let track = DeclaredName::new("veterancy").expect("a track's name");
        for (name, (thresholds, starts)) in trained {
            let own = (thresholds != common).then_some(thresholds);
            let entry = TypeTrackData {
                levels: own,
                starts,
                ..TypeTrackData::from(track.clone())
            };
            import.tracks.insert(name, TypeTracksData(vec![entry]));
        }
        let modifiers: Vec<DeclaredName> = LEVEL_MODIFIERS
            .map(|name| DeclaredName::new(name).expect("a modifier's name"))
            .to_vec();
        for (name, bonus) in modifiers.iter().zip(&game_data.veterancy) {
            let modifier =
                LevelModifier::of(*bonus, names).map_err(|_| ZeroHourError::VeterancyBonus)?;
            import.modifiers.insert(name.clone(), modifier);
        }
        let data = TrackData {
            modifiers,
            points: Some(vec![0; 4]),
            ..TrackData::new(common, true)
        };
        import.share = Some(ShareData {
            track: track.clone(),
            to: ExperienceTo::Killer {
                killer: names.hostiles.clone(),
            },
            victim: names.built.clone(),
        });
        import.track = Some((track, data));
        Ok(import)
    }

    /// The starts of `experience`'s `VeterancyGainCreate` modules past regular, each at the
    /// engine's level, one past the game's, by the modifier of its science's perk, when it names
    /// one; an error for a science's name past the most a name holds.
    fn starts(experience: &IniExperience) -> Result<Vec<StartData>, NameError> {
        let starts = experience.starts.iter().filter(|start| start.level > 0);
        starts
            .map(|start| {
                let requires = start
                    .science
                    .as_deref()
                    .map(|science| ImportName::latin(science).declared())
                    .transpose()?;
                Ok(StartData {
                    level: Level::new(u32::from(start.level) + 1).expect("a level past regular"),
                    requires,
                })
            })
            .collect()
    }

    /// The thresholds of levels 2 to 4 of `experience`, of a first of 0; none for a list that
    /// does not start at 0 or rise past it in each step, which the engine's tracks do not hold.
    fn thresholds(experience: &IniExperience) -> Option<Thresholds> {
        let [first, rest @ ..] = experience.required;
        if first != 0 {
            return None;
        }
        Thresholds::new(rest.map(|required| Num::int(i64::from(required))))
    }

    /// The thresholds most of `thresholds` are, the least of those as common at a tie; none of
    /// none.
    fn commonest<'a>(thresholds: impl Iterator<Item = &'a Thresholds>) -> Option<Thresholds> {
        let mut counts = BTreeMap::<&[Num], usize>::new();
        for thresholds in thresholds {
            *counts.entry(thresholds.get()).or_default() += 1;
        }
        let most = counts.values().copied().max()?;
        let (common, _) = counts.into_iter().find(|&(_, count)| count == most)?;
        Thresholds::new(common.iter().copied())
    }
}

impl LevelModifier {
    /// The modifier of a level of `bonus`: what it adds to the damage and the rate of fire of
    /// every weapon, its bonus less 1, as `WeaponBonusSet::appendBonuses` adds it, and the share
    /// it raises the most life by, its health bonus less 1, each in the game's `f32`, and none of
    /// a bonus of 1.
    fn of(bonus: VeterancyBonus, names: &RulesNames) -> Result<Self, FloatError> {
        let less_one = |factor: f32| -> Result<Option<Number>, FloatError> {
            let added = game_float::to_num(factor - 1.0)?;
            Ok((added != Num::ZERO).then_some(Number::Value(Scalar::Decimal(added))))
        };
        let stats = [
            (
                &names.veterancy_damage,
                less_one(bonus.damage)?.map(Change::Add),
            ),
            (
                &names.veterancy_rate,
                less_one(bonus.rate_of_fire)?.map(Change::Add),
            ),
            (&names.life_stat, less_one(bonus.health)?.map(Change::Pct)),
        ];
        let stats = stats
            .into_iter()
            .filter_map(|(stat, change)| Some((stat.clone(), change?)));
        Ok(Self {
            stats: stats.collect(),
        })
    }
}

#[cfg(test)]
mod tests;
