use std::collections::BTreeMap;
use std::num::{NonZeroU8, NonZeroU32};

use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_progression::{ExperienceTo, PerkData, ShareData, Thresholds, TrackData, TrackHolder};
use campfire_stats::Level;
use campfire_values::{FilterTag, TagFilterData};

use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::ini::command_button_ini::ButtonCommand;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::skirmish_start::FactionStart;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// The name of the general's rank, the mode's player track.
const TRACK: &str = "rank";

/// The lead of the name of a rank's science, `SCIENCE_Rank<n>`, which a science names among its
/// prerequisites for the rank it needs.
const RANK_LEAD: &str = "SCIENCE_Rank";

/// The import's general's ranks and sciences (04-capabilities/progression.md, A track of players;
/// 04-capabilities/progression.md, A science is a perk of a player): the player track `rank`, of
/// `Rank.ini`'s skill points from rank 2 and each rank's points to buy sciences, and its share, to
/// the killer's player of a death of an enemy's unit but a site's, as
/// `Player::addSkillPointsForKill` counts none for a victim `UNDER_CONSTRUCTION` and
/// `Object::scoreTheKill` none for the neutral player; the perks, one for each science a playable
/// template holds from the start or buys in its purchase sets, each one a train, a build, a
/// research or a start needs, and each prerequisite of one but a rank's, each of its cost, none for
/// a cost of 0, the level of its highest rank's science, its other prerequisites, and a modifier of
/// its name; and each faction's intrinsic sciences, which the mode's start grants, and the skill
/// sets of its side, which its bot buys (09-zero-hour.md, A bot buys its skill set). None of a game
/// of no ranks.
#[derive(Debug, Default)]
pub(crate) struct ScienceImport {
    pub(crate) track: Option<(DeclaredName, TrackData)>,
    pub(crate) share: Option<ShareData>,
    pub(crate) perks: BTreeMap<DeclaredName, PerkData>,
    pub(crate) intrinsic: BTreeMap<DeclaredName, Vec<DeclaredName>>,
    pub(crate) skill_sets: BTreeMap<DeclaredName, Vec<Vec<DeclaredName>>>,
}

/// A perk's button on a page of its faction's panel of perks: its place, from 1, its perk, and its
/// button's image, by name, none for a button of none (02-engine-core.md, The perk panel by a
/// choice).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PageFace {
    pub(crate) place: NonZeroU8,
    pub(crate) perk: DeclaredName,
    pub(crate) image: Option<String>,
}

impl ScienceImport {
    /// The pages of each faction's panel of perks, one for each of its template's purchase sets,
    /// in their order, each purchase of the set at its place, as `ControlBar` lays the sets'
    /// windows; an error for a science's name past the most a name holds.
    pub(crate) fn pages(
        rules: &ZeroHourRules,
    ) -> Result<BTreeMap<DeclaredName, Vec<Vec<PageFace>>>, ZeroHourError> {
        let mut pages = BTreeMap::new();
        for (template_name, template) in rules.templates.playable() {
            let faction = FactionStart::name_of(template_name)?;
            let mut held = Vec::new();
            for set in template.purchases.iter().flatten() {
                let mut page = Vec::new();
                for (place, button) in rules.sets.buttons(set).into_iter().flatten() {
                    let Some(button) = rules.buttons.get(button) else {
                        continue;
                    };
                    let science = button.sciences.first();
                    let (ButtonCommand::PurchaseScience, Some(science), Some(place)) =
                        (button.command, science, NonZeroU8::new(place))
                    else {
                        continue;
                    };
                    page.push(PageFace {
                        place,
                        perk: Self::perk_name(science)?,
                        image: button.image.clone(),
                    });
                }
                held.push(page);
            }
            pages.insert(faction, held);
        }
        Ok(pages)
    }

    /// The ranks and sciences of `rules`, `needed` among the sciences, those a train, a build, a
    /// research or a start needs; an error for ranks that do not rise from rank 1 at no points, a
    /// science that no file defines, and one that needs a rank past the last.
    pub(crate) fn of<'r>(
        rules: &'r ZeroHourRules,
        needed: impl Iterator<Item = &'r str>,
    ) -> Result<Self, ZeroHourError> {
        let ranks: Vec<_> = rules.ranks.ranks().collect();
        if ranks.is_empty() {
            return Ok(Self::default());
        }
        let numbered = ranks.iter().zip(1..).all(|(&(number, _), at)| number == at);
        if !numbered || ranks[0].1.needed != 0 {
            return Err(ZeroHourError::Ranks);
        }
        let skill = ranks[1..]
            .iter()
            .map(|(_, rank)| Num::int(i64::from(rank.needed)));
        let thresholds = Thresholds::new(skill).ok_or(ZeroHourError::Ranks)?;
        let track = DeclaredName::new(TRACK).expect("a track's name");
        let data = TrackData {
            holder: TrackHolder::Player,
            points: Some(ranks.iter().map(|(_, rank)| rank.points).collect()),
            ..TrackData::new(thresholds, false)
        };
        let victim = TagFilterData::new([FilterTag {
            name: DeclaredName::new("constructing").expect("the engine's tag"),
            negated: true,
        }]);
        let share = ShareData {
            track: track.clone(),
            to: ExperienceTo::Player {
                killer: rules.names.enemies.clone(),
            },
            victim,
        };
        let mut import = Self {
            track: Some((track.clone(), data)),
            share: Some(share),
            ..Self::default()
        };
        let mut wanted: Vec<&str> = needed.collect();
        for (template_name, template) in rules.templates.playable() {
            let faction = FactionStart::name_of(template_name)?;
            let intrinsic = template
                .intrinsic
                .iter()
                .map(|science| Self::perk_name(science))
                .collect::<Result<Vec<_>, _>>()?;
            import.intrinsic.insert(faction.clone(), intrinsic);
            wanted.extend(template.intrinsic.iter().map(String::as_str));
            let sets = template.side.iter().flat_map(|side| rules.ai.sets(side));
            let sets = sets.filter(|set| !set.is_empty()).map(|set| {
                wanted.extend(set.iter().map(String::as_str));
                set.iter()
                    .map(|science| Self::perk_name(science))
                    .collect::<Result<Vec<_>, _>>()
            });
            import
                .skill_sets
                .insert(faction, sets.collect::<Result<_, _>>()?);
            let sets = template.purchases.iter().flatten();
            let buttons = sets
                .filter_map(|set| rules.sets.buttons(set))
                .flatten()
                .filter_map(|(_, button)| rules.buttons.get(button))
                .filter(|button| button.command == ButtonCommand::PurchaseScience);
            wanted.extend(buttons.flat_map(|button| button.sciences.iter().map(String::as_str)));
        }
        while let Some(name) = wanted.pop() {
            let perk = Self::perk_name(name)?;
            if import.perks.contains_key(&perk) {
                continue;
            }
            let science = rules
                .sciences
                .named(name)
                .ok_or_else(|| ZeroHourError::NoScience(name.to_owned()))?;
            let mut rank = 1;
            let mut requires = Vec::new();
            for prerequisite in &science.prerequisites {
                if let Some(needed) = Self::rank_of(prerequisite) {
                    rank = rank.max(needed);
                } else {
                    requires.push(Self::perk_name(prerequisite)?);
                    wanted.push(prerequisite);
                }
            }
            if usize::try_from(rank).is_ok_and(|rank| rank > ranks.len()) {
                return Err(ZeroHourError::ScienceRank(name.to_owned()));
            }
            let level = Level::new(rank).filter(|level| level.get() > 1);
            import.perks.insert(
                perk.clone(),
                PerkData {
                    cost: NonZeroU32::new(science.cost),
                    requires,
                    track: level.map(|_| track.clone()),
                    level,
                    modifier: perk,
                },
            );
        }
        Ok(import)
    }

    /// The name of the perk, and of its modifier, of the science `science`.
    fn perk_name(science: &str) -> Result<DeclaredName, ZeroHourError> {
        Ok(ImportName::latin(science).declared()?)
    }

    /// The rank `science` grants, when it is a rank's science, `SCIENCE_Rank<n>`.
    fn rank_of(science: &str) -> Option<u32> {
        science.strip_prefix(RANK_LEAD)?.parse().ok()
    }
}

#[cfg(test)]
mod tests;
