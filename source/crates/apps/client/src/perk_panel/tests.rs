use std::collections::BTreeMap;
use std::num::{NonZeroU8, NonZeroU32};

use campfire_client_files::{PanelGrid, PerkPage, PerkPlace};
use campfire_math::Num;
use campfire_progression::{
    Experience, PerkData, Thresholds, TrackBook, TrackData, TrackHolder, TrackSet,
};
use campfire_stats::Level;
use campfire_values::{ModifierId, NameId, Names, PackageIndex, TrackId};

use super::*;

fn name(text: &str) -> DeclaredName {
    DeclaredName::new(text).unwrap()
}

/// A player track `rank` of level 2 at 50, and the perks `drill`, of cost 1, and `march`, of
/// cost 2, which requires `drill` and level 2.
fn book() -> (PerkBook, TrackBook) {
    let rank = TrackData {
        holder: TrackHolder::Player,
        points: Some(vec![1, 2]),
        ..TrackData::new(Thresholds::new([Num::int(50)]).unwrap(), false)
    };
    let tracks = TrackBook::new(&[(name("rank"), rank)].into()).unwrap();
    let mut modifiers = Names::<ModifierId>::default();
    for modifier in ["drill", "march"] {
        modifiers.add((PackageIndex::MODE, modifier));
    }
    let perk = |cost, modifier: &str| PerkData {
        cost: NonZeroU32::new(cost),
        requires: Vec::new(),
        track: None,
        level: None,
        modifier: name(modifier),
    };
    let march = PerkData {
        requires: vec![name("drill")],
        track: Some(name("rank")),
        level: Level::new(2),
        ..perk(2, "march")
    };
    let perks = BTreeMap::from([(name("drill"), perk(1, "drill")), (name("march"), march)]);
    (PerkBook::of(&perks, &tracks, &modifiers), tracks)
}

#[test]
fn a_faction_s_pages_lay_their_perks_in_their_places_order_page_after_page() {
    // America's two pages: `march` at 4 and `drill` at 1 of the first, `drill` again and a perk
    // the book lacks on the second; China's none.
    let (book, _) = book();
    let place = |at, perk: &str| PerkPlace {
        place: NonZeroU8::new(at).unwrap(),
        perk: name(perk),
        icon: None,
    };
    let file = PerksFile {
        choice: name("faction"),
        grid: PanelGrid::default(),
        pages: BTreeMap::from([(
            name("america"),
            vec![
                PerkPage {
                    places: vec![place(4, "march"), place(1, "drill")],
                },
                PerkPage {
                    places: vec![place(2, "drill"), place(1, "lost")],
                },
            ],
        )]),
    };
    let named = |text| book.names().named(text).unwrap();
    let [drill, march] = ["drill", "march"].map(named);
    assert_eq!(
        PerkPanelPlugin::laid(&file, &name("america"), &book),
        [drill, march, drill]
    );
    assert_eq!(PerkPanelPlugin::laid(&file, &name("china"), &book), []);
}

#[test]
fn a_perk_is_learnable_for_its_cost_held_or_dimmed_short_of_its_level() {
    // With 2 points: `drill` costs 1; held, it says so; `march` at rank 1 is locked, short of
    // its level, and at rank 2, beside `drill`, costs 2.
    let (book, tracks) = book();
    let named = |text| book.names().named(text).unwrap();
    let [drill, march] = ["drill", "march"].map(named);
    let rank = TrackId::at(0);
    let at_one = Experience::new(TrackSet::from([rank]));
    let mut at_two = at_one.clone();
    at_two.add(rank, Num::int(50), tracks.thresholds(rank));
    let learner = |perks, experience| Learner {
        perks,
        points: Some(2),
        experience: Some(experience),
    };
    let state = |perk, held| PerkPanelPlugin::state(&book, perk, held);
    let (none, drilled) = ([], [drill]);
    assert_eq!(
        state(drill, learner(&none, &at_one)),
        PerkState::Learnable(1)
    );
    assert_eq!(state(drill, learner(&drilled, &at_one)), PerkState::Held);
    assert_eq!(state(march, learner(&drilled, &at_one)), PerkState::Locked);
    assert_eq!(
        state(march, learner(&drilled, &at_two)),
        PerkState::Learnable(2)
    );
    // A click learns the perk by an order of no units.
    let order = PerkPanelPlugin::order(march);
    assert!(order.units.get().is_empty());
    assert_eq!(
        order.directive,
        Directive::from(ProgressionDirective::LearnPerk { perk: march })
    );
}
