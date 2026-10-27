use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::asset::Handle;
use bevy::math::Rect;
use campfire_actions::SlotKind;
use campfire_capabilities::{Directive, Match};
use campfire_client_files::{ButtonLook, ClientUnits, HudFile, PanelGrid, PlaceOrder};
use campfire_codec::Toml;
use campfire_declarations::Role;
use campfire_production::internals::TestBuild;
use campfire_production::{
    BuildStart, NewBuild, Production, Queued, RallyTarget, SiteStart, Style,
};
use campfire_sim::{Capability, IdAllocator, UnitDues};
use campfire_units::{BodyForm, BodyShape, GroundPoint, Layer, TypeOrigin};
use campfire_values::{Names, PackageIndex, Rank, Share};

use super::*;
use crate::command_panel::panel_looks::{FaceIcon, LookPlace};

/// A cell of the grid.
const fn cell(column: u8, row: u8) -> GridCell {
    GridCell { column, row }
}

#[test]
fn a_look_lays_its_buttons_at_its_places_and_one_of_none_its_trains_in_slot_order() {
    let mut world = Match::builder(&[Capability::Production, Capability::Stats])
        .role(Role::Client)
        .build();
    let (ranger, barracks, dozer) = (UnitType::new(1), UnitType::new(2), UnitType::new(3));
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
    let mut sales = ById::<UnitType, SellSpec>::default().reopen();
    let sale = SellSpec {
        refund: Box::new([]),
        time: Ticks::new(1),
    };
    sales.set(dozer, sale);
    let sales = sales.build();
    let books = MakingBooks {
        actions: world.resource::<ActionBook>(),
        builds: world.resource::<BuildBook>(),
        researches: world.get_resource::<ById<ActionId, ResearchSpec>>(),
        sales: Some(&sales),
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
    let d = Some(Key::KeyD);
    let faces = BTreeMap::from([(train, CellFace { icon: None, key: d })]);
    let action = |slot, action, making| Does::Action {
        slot,
        action,
        making,
    };
    let researched = Making::Research(ResearchSpec {
        modifier: drill,
        holder: ResearchHolder::Player,
    });

    // Of a look of no places, in Zero Hour's grid of 7 columns of 2, down each column: the
    // trains, builds and researches in slot order, from place 1 at the top left, place 2 under it
    // and place 3 at the top of the next column.
    let columns = PanelGrid {
        columns: NonZeroU8::new(7).unwrap(),
        rows: NonZeroU8::new(2).unwrap(),
        order: PlaceOrder::Columns,
        commands: BTreeMap::new(),
    };
    let looks = PanelLooks::new(columns.clone(), faces.clone(), Vec::new());
    let mut shown = Shown::default();
    let held = PanelUnit {
        unit_type: dozer,
        slots: Some(&slots),
        queue: Some(&queue),
        site: None,
    };
    assert!(shown.fill(center, held, books, &looks));
    assert_eq!(
        shown,
        Shown {
            unit: Some(center),
            buttons: vec![
                Button {
                    cell: cell(0, 0),
                    does: action(0, build, Making::Build(barracks)),
                    key: None,
                },
                Button {
                    cell: cell(0, 1),
                    does: action(1, train, Making::Train),
                    key: d,
                },
                Button {
                    cell: cell(1, 0),
                    does: action(2, research, researched),
                    key: None,
                },
            ],
            queue: vec![train, train],
            site: None,
        }
    );
    // A grid of one place lays the first alone.
    let one = PanelGrid {
        columns: NonZeroU8::MIN,
        rows: NonZeroU8::MIN,
        ..PanelGrid::default()
    };
    let looks = PanelLooks::new(one, faces.clone(), Vec::new());
    assert!(shown.fill(center, held, books, &looks));
    assert_eq!(shown.buttons.len(), 1);

    // The dozer's look places its build at 1, an action its slots do not hold at 2, its train at
    // 7, the 4th column's top, and the stop, the rally and the sale at 12, 13 and 14: each but the
    // unheld action, the rally as the dozer trains, the sale as its type sells.
    let at = |place: u8| columns.cell(NonZeroU8::new(place).unwrap()).unwrap();
    let placed = |place, button| LookPlace {
        cell: at(place),
        button,
    };
    let dozer_places = [
        placed(1, PlacedButton::Action(build)),
        placed(2, PlacedButton::Action(ActionId::new(99))),
        placed(7, PlacedButton::Action(train)),
        placed(12, PlacedButton::Command(PanelCommand::Stop)),
        placed(13, PlacedButton::Command(PanelCommand::Rally)),
        placed(14, PlacedButton::Command(PanelCommand::Sell)),
    ];
    let mut places: Vec<Box<[LookPlace]>> = vec![Box::new([]); 4];
    places[3] = Box::new(dozer_places);
    let looks = PanelLooks::new(columns, faces, places);
    assert!(shown.fill(center, held, books, &looks));
    let command = |cell, command| Button {
        cell,
        does: Does::Command(command),
        key: None,
    };
    let laid = vec![
        Button {
            cell: cell(0, 0),
            does: action(0, build, Making::Build(barracks)),
            key: None,
        },
        Button {
            cell: cell(3, 0),
            does: action(1, train, Making::Train),
            key: d,
        },
        command(cell(5, 1), PanelCommand::Stop),
        command(cell(6, 0), PanelCommand::Rally),
        command(cell(6, 1), PanelCommand::Sell),
    ];
    assert_eq!(shown.buttons, laid);
    // With no sales and no queue, neither the sale nor the rally.
    let unsold = MakingBooks {
        sales: None,
        ..books
    };
    let idle = PanelUnit {
        queue: None,
        ..held
    };
    assert!(shown.fill(center, idle, unsold, &looks));
    assert_eq!(shown.buttons, laid[..3]);

    // A unit of no train or build and no places shows nothing.
    let none = ActionSlots::new([]);
    let held = PanelUnit {
        unit_type: ranger,
        slots: Some(&none),
        queue: None,
        site: None,
    };
    assert!(!shown.fill(wall, held, books, &looks));
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
        unit_type: dozer,
        slots: Some(&slots),
        queue: Some(&queue),
        site: Some(&site),
    };
    assert!(shown.fill(wall, held, books, &looks));
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
fn the_looks_load_each_actions_face_and_refuse_a_place_past_the_grid_or_of_no_action() {
    // The barracks' look places the ranger's train at 1, a stop at 16, past the 15 places of the
    // engine's grid of 5 by 3, and an action no action of the match is at 2; the train's look
    // gives its icon of 60 by 48 pixels and its key, and the HUD file the sale's key.
    let text = r#"
        [units.barracks]
        draws = []
        panel = [
            { place = 1, action = "train_ranger" },
            { place = 2, action = "nothing" },
            { place = 16, command = "stop" },
        ]

        [actions.train_ranger]
        key = "KeyR"
        icon = { texture = "client/textures/atlas.ktx2", rect = [63, 301, 123, 349] }
    "#;
    let load = ClientLoad::of_looks([Toml::parse::<ClientUnits>(text).unwrap()]);
    let origins: TypeOriginBook = [TypeOrigin {
        package: PackageIndex::MODE,
        name: "barracks".into(),
    }]
    .into_iter()
    .collect();
    let mut actions = Names::<ActionId>::default();
    let train = actions.add((PackageIndex::MODE, "train_ranger"));
    let mut names = MatchNames::default();
    names.set_table(Arc::new(actions));
    let mut hud = HudFile::default();
    let sell = ButtonLook {
        icon: None,
        key: Some(Key::KeyS),
    };
    hud.panel.commands.insert(PanelCommand::Sell, sell);
    let text = EngineText::english();
    let sources = LookSources {
        origins: &origins,
        names: &names,
        load: &load,
        hud: &hud,
        text: &text,
    };
    let mut loaded = Vec::new();
    let looks = PanelLooks::of(sources, |package, path| {
        loaded.push((package, path.as_str().to_owned()));
        Handle::default()
    });
    let barracks = origins.iter().next().unwrap().0;
    assert_eq!(
        looks.places(barracks),
        [LookPlace {
            cell: cell(0, 0),
            button: PlacedButton::Action(train),
        }]
    );
    assert_eq!(
        looks.action(train),
        Some(&CellFace {
            icon: Some(FaceIcon {
                image: Handle::default(),
                rect: Rect::new(63.0, 301.0, 123.0, 349.0),
            }),
            key: Some(Key::KeyR),
        })
    );
    assert_eq!(
        loaded,
        [(PackageIndex::MODE, "client/textures/atlas.ktx2".to_owned())]
    );
    let sale = looks.command(PanelCommand::Sell);
    assert_eq!(
        (sale.face.key, sale.name.as_str()),
        (Some(Key::KeyS), "Sell")
    );
    let stop = looks.command(PanelCommand::Stop);
    assert_eq!((stop.face.key, stop.name.as_str()), (None, "Stop"));
}

#[test]
fn a_key_presses_the_first_shown_button_of_it_and_each_press_does_its_button_s_work() {
    // Three buttons of the key D, the second the first's twin, and a stop of S; a press of D and
    // of S takes each key once, and gives the first button of D and the stop.
    let mut ids = IdAllocator::default();
    let [dozer, tank] = [(); 2].map(|()| ids.allocate());
    let train = Does::Action {
        slot: 1,
        action: ActionId::new(0),
        making: Making::Train,
    };
    let keyed = |cell, does, key| Button { cell, does, key };
    let stop = keyed(
        cell(1, 0),
        Does::Command(PanelCommand::Stop),
        Some(Key::KeyS),
    );
    let first = keyed(cell(0, 0), train, Some(Key::KeyD));
    let shown = Shown {
        unit: Some(dozer),
        buttons: vec![
            first,
            keyed(
                cell(0, 1),
                Does::Command(PanelCommand::Sell),
                Some(Key::KeyD),
            ),
            stop,
            keyed(cell(2, 0), Does::Command(PanelCommand::Rally), None),
        ],
        ..Shown::default()
    };
    let mut keys = ButtonInput::<KeyCode>::default();
    keys.press(KeyCode::KeyD);
    keys.press(KeyCode::KeyS);
    let mut asked = Vec::new();
    let pressed: Vec<(usize, Button)> = shown
        .keyed(|key| {
            asked.push(key);
            InputMap::take_pressed(Chord::plain(key), &mut keys)
        })
        .collect();
    assert_eq!(pressed, [(0, first), (2, stop)]);
    assert_eq!(asked, [Key::KeyD, Key::KeyS]);
    // The keys are the panel's alone: the frame's other readers see neither go down.
    assert!(!keys.just_pressed(KeyCode::KeyD) && !keys.just_pressed(KeyCode::KeyS));
    // A key with a modifier presses nothing.
    let mut keys = ButtonInput::<KeyCode>::default();
    keys.press(KeyCode::ShiftLeft);
    keys.press(KeyCode::KeyD);
    let take = |key| InputMap::take_pressed(Chord::plain(key), &mut keys);
    assert_eq!(shown.keyed(take).count(), 0);

    // A train orders its unit; a stop and a sale the selection, none for an empty one; an attack
    // move and a rally aim, and the aim orders the selection at the point it picks.
    let mut selection = Selection::default();
    let angle = |_| Num::ZERO;
    let press = |does, selection: &Selection| {
        keyed(cell(0, 0), does, None).pressed(dozer, selection, angle)
    };
    let slot = OrdersDirective::Slot {
        slot: 1,
        target: ActionTarget::None,
    };
    assert_eq!(
        press(train, &selection),
        Some(Pressed::Order(Order::one(dozer, slot)))
    );
    assert_eq!(press(Does::Command(PanelCommand::Stop), &selection), None);
    selection.pick(&[tank, dozer], |_| None);
    let order = |directive: Directive| Pressed::Order(selection.order(directive).unwrap());
    assert_eq!(
        press(Does::Command(PanelCommand::Stop), &selection),
        Some(order(OrdersDirective::Stop.into()))
    );
    assert_eq!(
        press(Does::Command(PanelCommand::Sell), &selection),
        Some(order(ProductionDirective::Sell.into()))
    );
    assert_eq!(
        press(Does::Command(PanelCommand::AttackMove), &selection),
        Some(Pressed::Aim(AimedCommand::AttackMove))
    );
    assert_eq!(
        press(Does::Command(PanelCommand::Rally), &selection),
        Some(Pressed::Aim(AimedCommand::Rally))
    );
    let to = GroundPoint {
        x: Num::int(30),
        z: Num::int(-12),
    };
    assert_eq!(
        AimedCommand::AttackMove.directive(to),
        OrdersDirective::AttackMove { to }.into()
    );
    assert_eq!(
        AimedCommand::Rally.directive(to),
        ProductionDirective::Rally {
            target: Some(RallyTarget::Point(to)),
        }
        .into()
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
fn a_tooltip_shows_a_buttons_name_key_costs_and_whole_seconds_and_a_dimmed_one_is_unpaid() {
    // 150 ticks at 30 a second are 5 s; 151 round up to 6.
    let gold = ResourceAmount {
        resource: ResourceId::at(0),
        amount: 100,
    };
    let gems = ResourceAmount {
        resource: ResourceId::at(1),
        amount: 25,
    };
    let costs = [gold, gems];
    let seconds = |ticks: u64| ticks.div_ceil(30);
    let paid = ButtonValue::Costs {
        costs: &costs,
        seconds: seconds(150),
    };
    let tip = |name, key, value| ButtonTip { name, key, value }.to_string();
    assert_eq!(
        tip("Dozer", Some(Key::KeyD), paid),
        "Dozer  [D]  100  25  5 s"
    );
    let free = ButtonValue::Costs {
        costs: &[],
        seconds: seconds(151),
    };
    assert_eq!(tip("Wall", None, free), "Wall  6 s");
    assert_eq!(
        tip("Drill", Some(Key::Digit1), ButtonValue::Done),
        "Drill  [1]  done"
    );
    assert_eq!(tip("Sell", None, ButtonValue::Nothing), "Sell");
    // A train is pressed while its player pays its costs; a research held, an action unlearned
    // never; a command always.
    let train = Button {
        cell: cell(0, 0),
        does: Does::Action {
            slot: 0,
            action: ActionId::new(0),
            making: Making::Train,
        },
        key: None,
    };
    let rich = |costs: &[ResourceAmount]| costs.iter().all(|cost| cost.amount <= 100);
    let poor = |costs: &[ResourceAmount]| costs.is_empty();
    assert!(train.enabled(paid, rich));
    assert!(!train.enabled(paid, poor));
    assert!(!train.enabled(ButtonValue::Done, rich));
    assert!(!train.enabled(ButtonValue::Nothing, rich));
    let stop = Button {
        does: Does::Command(PanelCommand::Stop),
        ..train
    };
    assert!(stop.enabled(ButtonValue::Nothing, poor));
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
