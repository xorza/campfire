use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::asset::Handle;
use bevy::math::Rect;
use campfire_actions::SlotKind;
use campfire_capabilities::{Directive, Match};
use campfire_client_files::{ButtonLook, ClientUnits, HudFile, PanelGrid, PlaceOrder};
use campfire_codec::Toml;
use campfire_declarations::{ByIdBuilder, Role};
use campfire_math::Vec3;
use campfire_production::internals::TestBuild;
use campfire_production::{
    BuildStart, NewBuild, Production, Queued, RallyTarget, SiteStart, Style,
};
use campfire_sim::{Capability, IdAllocator, UnitDues};
use campfire_stats::TypeUpgrade;
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
        tags: TagSet::default(),
        riders: &[],
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

    // The dozer's look places its build at 1, an action its slots do not hold at 2, two exits at 4
    // and 5, its train at 7, the 4th column's top, an evacuation at 9, and the stop, the rally and
    // the sale at 12, 13 and 14: each but the unheld action, the exits and the evacuation, as it
    // holds no rider, the rally as the dozer trains, the sale as its type sells.
    let at = |place: u8| columns.cell(NonZeroU8::new(place).unwrap()).unwrap();
    let placed = |place, button| LookPlace {
        cell: at(place),
        button,
    };
    let dozer_places = [
        placed(1, PlacedButton::Action(build)),
        placed(2, PlacedButton::Action(ActionId::new(99))),
        placed(4, PlacedButton::Command(PanelCommand::Exit)),
        placed(5, PlacedButton::Command(PanelCommand::Exit)),
        placed(7, PlacedButton::Action(train)),
        placed(9, PlacedButton::Command(PanelCommand::Evacuate)),
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
    // Holding three riders, its two exits show the first two in the order they entered, each by
    // its type, and its evacuation shows.
    let (first, second, third) = (ids.allocate(), ids.allocate(), ids.allocate());
    let riders = [(first, ranger), (second, dozer), (third, ranger)];
    let holding = PanelUnit {
        riders: &riders,
        ..held
    };
    assert!(shown.fill(center, holding, books, &looks));
    let exit = |cell, rider, unit_type| Button {
        cell,
        does: Does::Exit { rider, unit_type },
        key: None,
    };
    let mut full = laid;
    full.insert(1, exit(cell(1, 1), first, ranger));
    full.insert(2, exit(cell(2, 0), second, dozer));
    full.insert(4, command(cell(4, 0), PanelCommand::Evacuate));
    assert_eq!(shown.buttons, full);

    // A unit of no train or build and no places shows nothing.
    let none = ActionSlots::new([]);
    let held = PanelUnit {
        unit_type: ranger,
        slots: Some(&none),
        queue: None,
        site: None,
        tags: TagSet::default(),
        riders: &[],
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
        tags: TagSet::default(),
        riders: &[],
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
    // An evacuation orders the selection; a rider's button its container, the dozer here, to let
    // that rider out.
    assert_eq!(
        press(Does::Command(PanelCommand::Evacuate), &selection),
        Some(order(OrdersDirective::Evacuate.into()))
    );
    let rider = Does::Exit {
        rider: tank,
        unit_type: UnitType::new(0),
    };
    assert_eq!(
        press(rider, &selection),
        Some(Pressed::Order(Order::one(
            dozer,
            OrdersDirective::Exit { rider: tank }
        )))
    );
    let to = GroundPoint {
        x: Num::int(30),
        z: Num::int(-12),
    };
    let at = Vec3::new(Num::int(30), Num::int(2), Num::int(-12));
    let ordered = |directive: Directive| selection.order(directive);
    assert_eq!(
        AimedCommand::AttackMove.order(to, at, &selection),
        ordered(OrdersDirective::AttackMove { to }.into())
    );
    assert_eq!(
        AimedCommand::Rally.order(to, at, &selection),
        ordered(
            ProductionDirective::Rally {
                target: Some(RallyTarget::Point(to)),
            }
            .into()
        )
    );
    // A cast aims too, and the aim orders its unit to cast at the point of the world it picks,
    // whatever the selection.
    let cast = Does::Action {
        slot: 2,
        action: ActionId::new(7),
        making: Making::Cast,
    };
    let aimed = AimedCommand::Cast {
        unit: dozer,
        slot: 2,
    };
    assert_eq!(press(cast, &selection), Some(Pressed::Aim(aimed)));
    let slot = OrdersDirective::Slot {
        slot: 2,
        target: ActionTarget::Point(at),
    };
    assert_eq!(
        aimed.order(to, at, &Selection::default()),
        Some(Order::one(dozer, slot))
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
    // An upgrade of `drill` that `walls` holds back: the player's `walls` is held back, as its
    // player holds `drill`; the player's `drill` is not, as no one holds `walls`.
    let upgrades = TypeUpgrades(Box::new([TypeUpgrade {
        triggers: Box::new([drill]),
        all: false,
        conflicts: Box::new([walls]),
        when: None,
        below: None,
        modifier: ModifierId::new(2),
    }]));
    let held_back =
        |making: Making| making.held_back(Some(&upgrades), |spec| done(Making::Research(spec)));
    assert!(held_back(research(walls, ResearchHolder::Player)));
    assert!(!held_back(research(drill, ResearchHolder::Player)));
    assert!(!held_back(Making::Train));
}

#[test]
fn a_cast_shows_while_its_when_passes_and_dims_while_it_cools() {
    // A cast of `when = "all:dead"` hides for a unit of no tag, and one of no `when` shows.
    let world = Match::builder(&[Capability::Stats])
        .role(Role::Client)
        .build();
    let dead = Filter::of_match(&world, "all:dead");
    assert_eq!(Making::cast(Some(dead), TagSet::default()), None);
    assert_eq!(Making::cast(None, TagSet::default()), Some(Making::Cast));
    // Cooling, it is dimmed; ready and afforded, it is not.
    let button = Button {
        cell: GridCell { column: 0, row: 0 },
        does: Does::Action {
            slot: 0,
            action: ActionId::new(0),
            making: Making::Cast,
        },
        key: None,
    };
    assert!(!button.enabled(ButtonValue::Cooling { seconds: 240 }, |_| true));
    let costs = ButtonValue::Costs {
        costs: &[],
        seconds: 0,
    };
    assert!(button.enabled(costs, |_| true));
    assert_eq!(
        ButtonValue::Cooling { seconds: 240 }.to_string(),
        "ready in 240 s"
    );
}

#[test]
fn a_train_at_a_place_is_dimmed_while_no_place_is_free_of_units_and_queued_trains() {
    // A train at a place, action 0, and one at the exit, action 1; two places, one held by a unit.
    let at_place = ActionId::new(0);
    let mut trains = ByIdBuilder::default();
    trains.set(
        at_place,
        TrainSpec {
            unit_type: UnitType::new(1),
            at_place: true,
        },
    );
    trains.set(
        ActionId::new(1),
        TrainSpec {
            unit_type: UnitType::new(2),
            at_place: false,
        },
    );
    let trains = trains.build();
    let held = HeldPlaces::of([Some(IdAllocator::default().allocate()), None]);
    let queued = |count: usize| {
        let mut queue = TrainQueue::default();
        let queued = Queued {
            action: at_place,
            rank: Rank::FIRST,
            time: Ticks::new(1),
            paid: 0,
        };
        for _ in 0..count {
            queue.push(queued, &[], Tick::new(0), &mut UnitDues::loose());
        }
        queue
    };
    let button = |action| Button {
        cell: GridCell { column: 0, row: 0 },
        does: Does::Action {
            slot: 0,
            action,
            making: Making::Train,
        },
        key: None,
    };
    let has_place = |action, queue: &TrainQueue| {
        button(action).has_place(Some(&trains), Some(&held), Some(queue))
    };
    // One place free: pressed with an empty queue, dimmed once a train at a place waits for it;
    // a train at the exit needs none.
    assert!(has_place(at_place, &queued(0)));
    assert!(!has_place(at_place, &queued(1)));
    assert!(has_place(ActionId::new(1), &queued(1)));
}

#[test]
fn a_train_of_a_modifier_its_player_lacks_is_dimmed() {
    // A train that requires the modifier 3, which its player lacks, then holds; an action that
    // requires nothing, and a command, need none.
    let mut world = Match::builder(&[Capability::Production, Capability::Stats])
        .role(Role::Client)
        .build();
    let train = Production::add_train(&mut world, UnitType::new(1), Ticks::new(1), None);
    let science = ModifierId::new(3);
    let mut requirements = RequirementBook::default();
    requirements.push(train, Vec::<[UnitType; 1]>::new(), [science]);
    let button = |action| Button {
        cell: GridCell { column: 0, row: 0 },
        does: Does::Action {
            slot: 0,
            action,
            making: Making::Train,
        },
        key: None,
    };
    let lacks = |_| false;
    assert!(!button(train).meets(Some(&requirements), lacks));
    assert!(button(train).meets(Some(&requirements), |held| held == science));
    assert!(button(ActionId::new(9)).meets(Some(&requirements), lacks));
    let stop = Button {
        does: Does::Command(PanelCommand::Stop),
        ..button(train)
    };
    assert!(stop.meets(Some(&requirements), lacks));
}
