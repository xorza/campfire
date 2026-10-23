use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, QueryData, Without};
use bevy_ecs::system::{Local, Query, Res};
use bevy_ecs::world::World;
use campfire_common::{PlayerSlot, Tick};
use campfire_declarations::ById;
use campfire_players::{PlayerResources, PlayerRowsMut};
use campfire_sim::{SimComponent, SimTick, StableId};
use campfire_states::{NotSent, Replication};
use campfire_stats::UnitStats;
use campfire_units::{Dead, Owner, Unfinished, UnitEntity};
use campfire_values::{NameId, ResourceId, UnitType};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::type_income::{TypeIncome, TypePays};

/// The clock of a unit's income (04-capabilities/games.md, Income over time): the tick its next
/// period ends in, the owner it pays, whose change is a capture, and where its capture's bonus
/// stands. State, of each unit of a type of an income, from its spawn.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncomeClock {
    next: Tick,
    owner: Option<PlayerSlot>,
    bonus: CaptureBonus,
}

/// Where a unit's capture bonus stands, as Zero Hour's `AutoDepositUpdate` awards its
/// `InitialCaptureBonus`: waiting for the end of the unit's first period, due from then to the
/// first capture by an owner, and paid after it, for good.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureBonus {
    Waiting,
    Due,
    Paid,
}

/// What the pass of incomes reads of a unit, and its clock, which it steps.
#[derive(QueryData)]
#[query_data(mutable)]
pub(crate) struct IncomeUnit {
    id: &'static StableId,
    clock: &'static mut IncomeClock,
    unit_type: &'static UnitType,
    owner: Option<&'static Owner>,
    unfinished: Has<Unfinished>,
    stats: Option<&'static UnitStats>,
}

/// What a clock pays in a tick: of the unit's period, and of its capture.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Paid {
    pub(crate) period: i64,
    pub(crate) capture: i64,
}

/// A payment of the tick's incomes: the unit's stable id, which orders them, its owner, and what
/// it pays in its resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Income {
    unit: StableId,
    player_slot: PlayerSlot,
    resource: ResourceId,
    amount: i64,
}

impl IncomeClock {
    /// The clock of a unit of `owner` that spawns at `now`, of a type of `period`.
    pub(crate) const fn spawned(now: Tick, owner: Option<PlayerSlot>, income: &TypeIncome) -> Self {
        Self {
            next: now.after(income.period),
            owner,
            bonus: CaptureBonus::Waiting,
        }
    }

    /// Steps the clock to `now`, of a unit of `owner` that is `finished` or not, of `income`,
    /// whose period pays `amount`: an owner other than its own is a capture, which starts the
    /// period again and pays the bonus while it is due; a unit left with no owner keeps its
    /// period, as `Player::becomingTeamMember` starts the deposit again for a player alone. The
    /// end of a period starts the next, makes a waiting bonus due, and pays an owner of a
    /// finished unit. What it pays, and whether it changed.
    pub(crate) fn step(
        &mut self,
        now: Tick,
        owner: Option<PlayerSlot>,
        finished: bool,
        income: &TypeIncome,
        amount: i64,
    ) -> Option<Paid> {
        let changed = owner != self.owner;
        self.owner = owner;
        if changed && owner.is_some() {
            self.next = now.after(income.period);
            let mut paid = Paid::default();
            if self.bonus == CaptureBonus::Due && income.capture > 0 {
                self.bonus = CaptureBonus::Paid;
                paid.capture = i64::from(income.capture);
            }
            return Some(paid);
        }
        if now < self.next {
            return changed.then_some(Paid::default());
        }
        self.next = now.after(income.period);
        if self.bonus == CaptureBonus::Waiting {
            self.bonus = CaptureBonus::Due;
        }
        let period = if owner.is_some() && finished {
            amount.max(0)
        } else {
            0
        };
        Some(Paid { period, capture: 0 })
    }

    /// Pays the tick's incomes, by the stable ids of their units: each unit of a type of an
    /// income steps its clock, and an owner takes what it pays, in the type's resource, a
    /// payment past what a resource holds not made, as a bounty's is not.
    pub(crate) fn pay(
        (tick, book): (Res<'_, SimTick>, Res<'_, ById<UnitType, TypeIncome>>),
        mut units: Query<'_, '_, IncomeUnit, Without<Dead>>,
        mut resources: PlayerRowsMut<'_, '_, PlayerResources>,
        mut incomes: Local<'_, Vec<Income>>,
    ) {
        let now = tick.start();
        incomes.clear();
        for mut held in &mut units {
            let (unit, unit_type, owner) = (*held.id, *held.unit_type, held.owner);
            let income = book
                .get(unit_type)
                .expect("a unit of an income clock has an income");
            let amount = match income.pays {
                TypePays::Amount(amount) => i64::from(amount),
                TypePays::Stat(stat) => {
                    let value = held
                        .stats
                        .and_then(|stats| stats.values().get(stat.index()));
                    value.map_or(0, |value| value.floor())
                }
            };
            let player_slot = owner.map(|owner| owner.player_slot());
            let mut stepped = *held.clock;
            let Some(paid) = stepped.step(now, player_slot, !held.unfinished, income, amount)
            else {
                continue;
            };
            *held.clock = stepped;
            let Some(player_slot) = player_slot else {
                continue;
            };
            for amount in [paid.period, paid.capture] {
                if amount > 0 {
                    incomes.push(Income {
                        unit,
                        player_slot,
                        resource: income.resource,
                        amount,
                    });
                }
            }
        }
        incomes.sort_unstable();
        for income in &*incomes {
            let owner = Owner::new(income.player_slot);
            if let Some(mut held) = resources.of_mut(Some(&owner)) {
                held.gain(income.resource, income.amount);
            }
        }
    }
}

impl SimComponent for IncomeClock {
    const NAME: &'static str = "mode.income_clock";
    type Host = UnitEntity;

    // A clock of a unit of no income has no period, and one of another owner than the unit's
    // would pay a capture the unit never had.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let unit = world.entity(entity);
        let typed = unit.get::<UnitType>().is_some_and(|&unit_type| {
            let book = world.get_resource::<ById<UnitType, TypeIncome>>();
            book.is_some_and(|book| book.get(unit_type).is_some())
        });
        typed && unit.get::<Owner>().map(|owner| owner.player_slot()) == self.owner
    }
}

impl Replication for IncomeClock {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}
