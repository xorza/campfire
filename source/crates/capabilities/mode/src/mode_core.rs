use std::num::NonZeroU8;

use campfire_actions::{LoadoutRanks, SlotClaims, SlotError, SlotKind, SlotKinds};
use campfire_common::DeclaredName;
use campfire_declarations::{
    BookCx, BookError, BookPlace, ById, Declarations, InstallCx, Module, NameSets, NoSystems,
    SourceKind,
};
use campfire_names::UnknownName;
use campfire_navigation::{ModeNavigationData, NavigationBook, WalkCheck, Walker, WalkerBook};
use campfire_script::Audience;
use campfire_sim::Capability;
use campfire_units::{BodyForm, MapBook, Variation, Variations, WallBook};
use campfire_values::{Named, PackageIndex, Team, UnitType};

use crate::bounty_book::BountyBook;
use crate::bounty_data::{BountyData, ModeBountyData};
use crate::choice_data::Offers;
use crate::error::ModeDataError;
use crate::income_data::IncomeData;
use crate::input_book::InputBook;
use crate::map_data::MapData;
use crate::marker_tag::MarkerTag;
use crate::match_end::MatchEnd;
use crate::mode_data::ModeData;
use crate::mode_map::ModeMap;
use crate::mode_queue::ModeQueues;
use crate::mode_units::ModeUnits;
use crate::player_control::PlayerControl;
use crate::player_slot_picks::PlayerSlotPicks;
use crate::type_income::TypeIncome;
use crate::victory::VictoryBook;
use crate::victory_data::VictoryData;

/// The part of the mode every match installs, whatever its role: the match's end, and the stop
/// of every stage once it comes, as a client holds the end the server sends. The scripted part,
/// `Mode`, installs once the scripts compile.
#[derive(Debug)]
pub struct ModeCore;

impl Module for ModeCore {
    const CAPABILITY: Option<Capability> = Some(Capability::Mode);
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[Capability::Navigation];
    type Systems = NoSystems;

    fn declare<D: Declarations>(d: &mut D) {
        d.names::<Team>(ModeMap::team_table);
        d.own::<ModeQueues>();
        d.fact::<LoadoutRanks>(Self::loadout_ranks);
        d.fact::<ModeMap>(ModeMap::build);
        d.extend::<NameSets>(Self::marker_tags);
        d.fact::<ModeUnits>(ModeUnits::build);
        d.section::<VictoryData>();
        d.section::<ModeBountyData>();
        d.section::<BountyData>();
        d.book::<BountyBook>(BountyBook::build);
        d.section::<IncomeData>();
        d.book::<ById<UnitType, TypeIncome>>(TypeIncome::book);
        d.book::<VictoryBook>(VictoryBook::build);
        d.book::<InputBook>(InputBook::build);
        d.rule(Self::data_rule);
        d.rule(Self::choices_rule);
        d.rule(Self::walk_rule);
        d.component::<MatchEnd>();
        d.stages_run_if(MatchEnd::is_running);
    }

    fn install(_: &mut InstallCx<'_, Self::Systems>) {}
}

impl ModeCore {
    /// Adds to `sets` the tags of the markers the mode's units spawn at and of the map's markers.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    fn marker_tags(cx: &BookCx<'_>, sets: &mut NameSets) -> Result<(), BookError> {
        let spawns = cx.mode_section::<ModeNavigationData>().into_iter();
        let spawns = spawns.flat_map(|navigation| &navigation.spawn_tags);
        let map = cx
            .given::<MapData>()
            .expect("the load gives the mode's map");
        let marked = map.markers.iter().flat_map(|marker| &marker.tags);
        sets.extend(MarkerTag::NAMES, spawns.chain(marked));
        Ok(())
    }

    /// The rules of the mode's own data: a bot takes a leaver's place only where players join
    /// late, a player controls an avatar only where the mode has one, no field of its state goes
    /// to an owner or a team, and no option shares a param's name.
    fn data_rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        let data = cx
            .given::<ModeData>()
            .expect("the load gives the mode's data");
        let fail = |at, problem| Err(BookError::rule(PackageIndex::MODE, at, problem));
        let entry = |table, name: &DeclaredName| BookPlace::Entry {
            table,
            name: name.clone(),
        };
        if data.players.bot_takeover && !data.players.late_join {
            let at = BookPlace::Section(ModeData::PLAYERS);
            return fail(at, ModeDataError::BotTakeoverWithoutLateJoin);
        }
        let avatars =
            || (cx.packages()).any(|(_, package)| matches!(package.kind, SourceKind::Avatar(_)));
        if data.players.control == PlayerControl::Avatar && !avatars() {
            let at = BookPlace::Section(ModeData::PLAYERS);
            return fail(at, ModeDataError::AvatarControlWithoutAvatars);
        }
        let owned = data
            .state
            .iter()
            .find(|(_, field)| matches!(field.audience, Audience::Owner | Audience::Team));
        if let Some((name, _)) = owned {
            return fail(entry(ModeData::STATE, name), ModeDataError::StateAudience);
        }
        let shared = data
            .options
            .keys()
            .find(|name| data.params.contains_key(*name));
        if let Some(name) = shared {
            return fail(entry(ModeData::OPTIONS, name), ModeDataError::OptionParam);
        }
        Ok(())
    }

    /// The rule of the mode's choices: a choice of loadout entries fills a slot kind the mode
    /// declares, which no capability claims, and a choice of avatars or values none; every choice
    /// of loadout entries fills a kind of the same ranks, which the entries have; a choice of
    /// values lists them, each once and fewer than 255, and takes one, and no other choice lists
    /// values.
    fn choices_rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        let data = cx
            .given::<ModeData>()
            .expect("the load gives the mode's data");
        let claims = cx.fact::<SlotClaims>()?;
        let kinds = cx.mode_section::<SlotKinds>();
        let mut ranks = None;
        for (name, choice) in &data.choices {
            let at = || BookPlace::Entry {
                table: ModeData::CHOICES,
                name: name.clone(),
            };
            let listed = match choice.offers {
                Offers::Values => {
                    let mut names: Vec<&DeclaredName> = choice.values.iter().collect();
                    names.sort_unstable();
                    names.dedup();
                    // Each value's place is a player slot pick's byte, short of its none.
                    let fits = choice.values.len() < usize::from(PlayerSlotPicks::NONE);
                    !choice.values.is_empty()
                        && fits
                        && names.len() == choice.values.len()
                        && choice.count == NonZeroU8::MIN
                }
                Offers::Avatars | Offers::Loadout => choice.values.is_empty(),
            };
            if !listed {
                let problem = ModeDataError::ChoiceValues;
                return Err(BookError::rule(PackageIndex::MODE, at(), problem));
            }
            match (choice.offers, &choice.slot) {
                (Offers::Loadout, Some(kind)) => {
                    let found =
                        kinds.and_then(|kinds| Some((kinds, kinds.names().named(kind.as_str())?)));
                    let (kinds, id) = found.ok_or_else(|| {
                        let unknown = UnknownName::new(SlotKind::NAMES, kind);
                        BookError::unknown(PackageIndex::MODE, at(), unknown)
                    })?;
                    if let Some(by) = claims.claimant(kind) {
                        let kind = kind.clone();
                        let problem = SlotError::ClaimedSlotted { kind, by };
                        return Err(BookError::rule(PackageIndex::MODE, at(), problem));
                    }
                    let kind_ranks = kinds.ranks(id);
                    if *ranks.get_or_insert(kind_ranks) != kind_ranks {
                        let at = BookPlace::Section(ModeData::CHOICES);
                        let problem = ModeDataError::LoadoutRanks;
                        return Err(BookError::rule(PackageIndex::MODE, at, problem));
                    }
                }
                (Offers::Avatars | Offers::Values, None) => {}
                _ => {
                    let problem = ModeDataError::ChoiceSlot;
                    return Err(BookError::rule(PackageIndex::MODE, at(), problem));
                }
            }
        }
        Ok(())
    }

    /// The rule of the mode's map: the kinds of its unit types that walk walk it, among its
    /// placed units of the types that do not walk, as `WalkCheck::run` says.
    fn walk_rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        let map = cx.fact::<ModeMap>()?;
        let walkers = cx.book_if::<ById<UnitType, Walker>>()?;
        let no_kinds = WalkerBook::default();
        let kinds = &cx.book_if::<WalkerBook>()?.unwrap_or(&no_kinds).kinds;
        let bodies = cx.book::<ById<UnitType, BodyForm>>()?;
        let variations = cx.book_if::<ById<UnitType, Variations>>()?;
        // A placed unit of a given variation stands as its body, and one of none as its type's,
        // which every variation fits.
        let body_of = |unit_type, variation: Option<Variation>| {
            assert!(
                cx.type_of(unit_type).stands(),
                "a placed unit's type stands"
            );
            let walks = walkers.is_some_and(|walkers| walkers.get(unit_type).is_some());
            let body = match variation {
                Some(variation) => variations
                    .and_then(|book| book.get(unit_type))
                    .and_then(|of_type| of_type.get(variation.get())),
                None => bodies.get(unit_type),
            };
            body.copied().filter(|_| !walks)
        };
        let name_of = |unit_type: UnitType| {
            DeclaredName::new(cx.type_of(unit_type).name).expect("a type's name is declared")
        };
        let none = NavigationBook::default();
        let navigation = cx.book_if::<NavigationBook>()?.unwrap_or(&none);
        let no_walls = WallBook::default();
        let walls = cx.book_if::<WallBook>()?.unwrap_or(&no_walls);
        let spawn_tags = cx.mode_section::<ModeNavigationData>();
        let spawn_tags = spawn_tags.map_or(&[][..], |navigation| &navigation.spawn_tags);
        let check = WalkCheck {
            ground: cx.book::<MapBook>()?,
            navigation,
            walls,
            walkers: kinds,
            placed: &map.placed_bodies(body_of),
            spawns: &map.spawn_points(spawn_tags),
        };
        let walked = check.run(name_of);
        walked.map_err(|problem| BookError::rule(PackageIndex::MODE, BookPlace::Map, problem))
    }

    /// The ranks of every loadout entry, as the mode's data gives them.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    fn loadout_ranks(cx: &BookCx<'_>) -> Result<LoadoutRanks, BookError> {
        let data = cx
            .given::<ModeData>()
            .expect("the load gives the mode's data");
        Ok(LoadoutRanks(data.loadout_ranks()))
    }
}
