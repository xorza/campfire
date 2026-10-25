use campfire_actions::SlotKind;
use campfire_capabilities::Match;
use campfire_declarations::Role;
use campfire_production::internals::TestBuild;
use campfire_production::{BuildStart, NewBuild, Production, Queued, SiteStart, Style};
use campfire_sim::{Capability, IdAllocator, UnitDues};
use campfire_units::{BodyForm, BodyShape, Layer};
use campfire_values::Rank;
use campfire_values::Share;

use super::*;

#[test]
fn the_panel_shows_a_producers_trains_builds_and_researches_its_queue_and_a_site() {
    let mut world = Match::builder(&[Capability::Production, Capability::Stats])
        .role(Role::Client)
        .build();
    let (ranger, barracks) = (UnitType::new(1), UnitType::new(2));
    let time = Ticks::new(150);
    let train = Production::add_train(&mut world, ranger, time, None);
    let build = TestBuild {
        range: Num::int(10),
        time: Ticks::new(300),
        resource_cost: None,
        build: NewBuild {
            unit_type: barracks,
            form: BodyForm::new(
                BodyShape::box_sized([Num::int(4), Num::int(2)]).unwrap(),
                Layer::FIRST,
            ),
            start: BuildStart::Arrival,
            style: Style::Builder,
            start_life: None,
            refund: Share::ALL,
            near: Vec::new(),
            away: Vec::new(),
        },
    };
    let build = Production::add_build(&mut world, build);
    let drill = ModifierId::new(0);
    let research = Production::add_research(&mut world, drill, ResearchHolder::Player, time, None);
    let books = MakingBooks {
        actions: world.resource::<ActionBook>(),
        builds: world.resource::<BuildBook>(),
        researches: world.get_resource::<ById<ActionId, ResearchSpec>>(),
    };
    let kind = SlotKind::new(0);
    let rank = Some(Rank::FIRST);
    // A dozer's slots: a build in slot 0, a train in slot 1 and a research in slot 2.
    let slots = ActionSlots::new([
        (build, kind, rank),
        (train, kind, rank),
        (research, kind, rank),
    ]);
    let mut queue = TrainQueue::default();
    let queued = Queued {
        action: train,
        rank: Rank::FIRST,
        time,
        paid: 0,
    };
    queue.push(queued, &[], Tick::new(0), &mut UnitDues::loose());
    queue.push(queued, &[], Tick::new(0), &mut UnitDues::loose());
    let mut ids = IdAllocator::default();
    let (center, wall) = (ids.allocate(), ids.allocate());
    let mut shown = Shown::default();
    let held = PanelUnit {
        slots: Some(&slots),
        queue: Some(&queue),
        site: None,
    };
    assert!(shown.fill(center, held, books));
    assert_eq!(
        shown,
        Shown {
            unit: Some(center),
            buttons: vec![
                Button {
                    slot: 0,
                    action: build,
                    making: Making::Build(barracks),
                },
                Button {
                    slot: 1,
                    action: train,
                    making: Making::Train,
                },
                Button {
                    slot: 2,
                    action: research,
                    making: Making::Research(ResearchSpec {
                        modifier: drill,
                        holder: ResearchHolder::Player,
                    }),
                },
            ],
            queue: vec![train, train],
            site: None,
        }
    );
    // A unit of no train or build shows nothing.
    let none = ActionSlots::new([]);
    let held = PanelUnit {
        slots: Some(&none),
        queue: None,
        site: None,
    };
    assert!(!shown.fill(wall, held, books));
    assert_eq!(shown, Shown::default());
    // A site shows its row alone, though its type trains.
    let site = Site::new(SiteStart {
        action: build,
        rank: Rank::FIRST,
        time: Num::int(4),
        gain: Num::ZERO,
        paid: &[],
        holder: None,
    });
    let held = PanelUnit {
        slots: Some(&slots),
        queue: Some(&queue),
        site: Some(&site),
    };
    assert!(shown.fill(wall, held, books));
    assert_eq!(
        shown,
        Shown {
            unit: Some(wall),
            site: Some(build),
            ..Shown::default()
        }
    );
}

#[test]
fn progress_shows_in_whole_percent_rounded_down() {
    // A head of 150 ticks from tick 0, done at 150: at 60, 60 / 150 = 40 %; at 149, 99.3 %
    // shows 99; at 150 and past, 100.
    let train = ActionId::new(0);
    let queued = Queued {
        action: train,
        rank: Rank::FIRST,
        time: Ticks::new(150),
        paid: 0,
    };
    let mut queue = TrainQueue::default();
    assert_eq!(head_percent(&queue, Tick::new(0)), None);
    queue.push(queued, &[], Tick::new(0), &mut UnitDues::loose());
    let at = |now| head_percent(&queue, Tick::new(now));
    assert_eq!(
        [at(0), at(60), at(149), at(150), at(400)],
        [0, 40, 99, 100, 100].map(Some)
    );
    // A head that reached the head at 100, after another: at 160, 60 of its 150 ticks.
    let mut queue = TrainQueue::default();
    queue.push(queued, &[], Tick::new(100), &mut UnitDues::loose());
    assert_eq!(head_percent(&queue, Tick::new(160)), Some(40));
    // A site 0.7 of the way to 300 ticks: 210 / 300 = 70 % exactly; one 2⁻³² below it, 69.
    let time = Num::int(300);
    let progress = Num::int(210);
    assert_eq!(site_percent(progress, time), 70);
    assert_eq!(
        site_percent(Num::from_bits(progress.to_bits() - 1), time),
        69
    );
    assert_eq!(site_percent(Num::ZERO, time), 0);
    assert_eq!(site_percent(time, time), 100);
    assert_eq!(site_percent(Num::ZERO, Num::ZERO), 100);
}

#[test]
fn a_button_shows_its_costs_and_its_whole_seconds_or_that_it_is_done() {
    // 150 ticks at 30 a second are 5 s; 151 round up to 6.
    let gold = ResourceAmount {
        resource: ResourceId::at(0),
        amount: 100,
    };
    let gems = ResourceAmount {
        resource: ResourceId::at(1),
        amount: 25,
    };
    let text = |value: ButtonValue<'_>| {
        let mut line = String::new();
        value.write(&mut line);
        line
    };
    let costs = [gold, gems];
    let seconds = |ticks: u64| ticks.div_ceil(30);
    assert_eq!(
        text(ButtonValue::Costs {
            costs: &costs,
            seconds: seconds(150),
        }),
        "100  25  5 s"
    );
    assert_eq!(
        text(ButtonValue::Costs {
            costs: &[],
            seconds: seconds(151),
        }),
        "6 s"
    );
    assert_eq!(text(ButtonValue::Done), "done");
    assert_eq!(text(ButtonValue::Unlearned), "");
}

#[test]
fn a_research_is_done_as_its_player_or_its_unit_from_itself_holds_it() {
    // The player holds the drill; the unit 1 carries the walls from itself, and the drill
    // from unit 2.
    let [drill, walls] = [0, 1].map(ModifierId::new);
    let mut ids = IdAllocator::default();
    let [_, unit, other] = [(); 3].map(|()| ids.allocate());
    let player_holds = |modifier| modifier == drill;
    let unit_holds = |modifier, source| {
        (modifier == walls && source == unit) || (modifier == drill && source == other)
    };
    let research = |modifier, holder| Making::Research(ResearchSpec { modifier, holder });
    let done = |making: Making| making.done(unit, player_holds, unit_holds);
    assert!(done(research(drill, ResearchHolder::Player)));
    assert!(!done(research(walls, ResearchHolder::Player)));
    assert!(done(research(walls, ResearchHolder::Producer)));
    assert!(!done(research(drill, ResearchHolder::Producer)));
    assert!(!done(Making::Train));
}
