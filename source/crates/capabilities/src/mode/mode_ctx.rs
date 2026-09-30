use std::cell::{RefCell, RefMut};
use std::rc::Rc;

use campfire_script::rhai::{Array, Dynamic, Engine, INT, ImmutableString};
use campfire_sim::{PlayerSlot, Position, StableId, Ticks};

use crate::mode::mode_book::ModeBook;
use crate::mode::picks::Picks;
use crate::mode::player_resources::PlayerResources;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::state_decl::StateType;
use crate::scripts::state_value::StateValue;
use crate::units::lane::Lane;
use crate::units::script_view::View;
use crate::units::team::Team;
use crate::units::unit::Unit;
use crate::units::unit_type::UnitType;

/// `ctx` in the mode's script: its params and state, the players' choices and resources, and
/// the spawns and timers one call queues. State and choices a call writes it reads back; they
/// commit, and its effects apply in the order queued, only after it returns successfully, so a
/// failed call changes nothing.
#[derive(Debug, Clone)]
pub(crate) struct ModeCtx {
    frame: Rc<RefCell<ModeFrame>>,
    view: View,
    book: Rc<ModeBook>,
}

/// What the running call reads and writes: copies of the mode's state, taken as it began, and
/// the effects it queued. One frame serves the whole match, its buffers filled again for each
/// call.
#[derive(Debug, Default)]
pub(crate) struct ModeFrame {
    pub(crate) state: Vec<StateValue>,
    pub(crate) picks: Picks,
    pub(crate) resources: PlayerResources,
    pub(crate) effects: Vec<ModeEffect>,
}

/// An effect a mode call queued.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ModeEffect {
    Timer {
        name: String,
        ticks: Ticks,
        repeat: bool,
        data: Option<StateValue>,
    },
    SpawnHeroes,
    SpawnUnit {
        unit_type: UnitType,
        team: Team,
        pos: Position,
    },
    SpawnWave {
        team: Team,
        lane: Lane,
        types: Vec<UnitType>,
    },
    Respawn {
        unit: StableId,
        ticks: Ticks,
    },
}

/// `ctx.p`: the mode's params, by name.
#[derive(Debug, Clone)]
pub(crate) struct ModeParams(ModeCtx);

/// `ctx.state`: the mode's state fields, by name, to read and write.
#[derive(Debug, Clone)]
pub(crate) struct ModeStateAccess(ModeCtx);

impl ModeCtx {
    pub(crate) fn new(view: View, book: ModeBook) -> ModeCtx {
        ModeCtx {
            frame: Rc::default(),
            view,
            book: Rc::new(book),
        }
    }

    /// The frame, borrowed until the guard drops. A call borrows it again, so no guard may live
    /// across a call.
    pub(crate) fn frame(&self) -> RefMut<'_, ModeFrame> {
        self.frame.borrow_mut()
    }

    pub(crate) const fn view(&self) -> &View {
        &self.view
    }

    pub(crate) fn book(&self) -> &ModeBook {
        &self.book
    }

    /// The script API of the mode's hooks.
    pub(crate) fn register(engine: &mut Engine) {
        ModeCtx::register_accessors(engine);
        engine
            .register_type_with_name::<ModeCtx>("ModeCtx")
            .register_get("p", |ctx: &mut ModeCtx| ModeParams(ctx.clone()))
            .register_get("state", |ctx: &mut ModeCtx| ModeStateAccess(ctx.clone()))
            .register_get("teams", |ctx: &mut ModeCtx| -> Array {
                let names = ctx.book.teams.playing();
                names
                    .map(|name| Dynamic::from(ImmutableString::from(name)))
                    .collect()
            })
            .register_get("players", |ctx: &mut ModeCtx| {
                INT::from(ctx.book.teams.players())
            })
            .register_get("map", |ctx: &mut ModeCtx| ctx.book.map())
            .register_fn(
                "enemy_team",
                |ctx: &mut ModeCtx, team: &str| -> Checked<Dynamic> {
                    let team = ctx.team(team)?;
                    let enemy = ctx
                        .book
                        .teams
                        .sole_enemy(team)
                        .ok_or_else(|| ApiError::NoEnemyTeam.fail())?;
                    ctx.view.team_name(enemy)
                },
            )
            .register_fn("heroes", |ctx: &mut ModeCtx| ctx.view.heroes(None))
            .register_fn(
                "heroes",
                |ctx: &mut ModeCtx, team: &str| -> Checked<Array> {
                    Ok(ctx.view.heroes(Some(ctx.team(team)?)))
                },
            )
            .register_fn("units_tagged", |ctx: &mut ModeCtx, tag: &str| {
                ctx.view.units_tagged(tag)
            })
            .register_fn(
                "hero_available",
                |ctx: &mut ModeCtx, player: INT, id: &str| ctx.hero_available(player, id),
            )
            .register_fn("choose_hero", |ctx: &mut ModeCtx, player: INT, id: &str| {
                ctx.choose_hero(player, id)
            })
            .register_fn(
                "choose_spells",
                |ctx: &mut ModeCtx, player: INT, ids: Array| ctx.choose_spells(player, &ids),
            )
            .register_fn("spawn_heroes", |ctx: &mut ModeCtx| {
                ctx.frame().effects.push(ModeEffect::SpawnHeroes);
            })
            .register_fn(
                "spawn_unit",
                |ctx: &mut ModeCtx, unit_type: &str, team: &str, pos: Position| {
                    ctx.spawn_unit(unit_type, team, pos)
                },
            )
            .register_fn(
                "spawn_wave",
                |ctx: &mut ModeCtx, team: &str, lane: &str, types: Array| {
                    ctx.spawn_wave(team, lane, &types)
                },
            )
            .register_fn(
                "timer",
                |ctx: &mut ModeCtx, name: &str, ms: INT, repeat: bool, data: Dynamic| {
                    ctx.timer(name, ms, repeat, &data)
                },
            )
            .register_fn("respawn", |ctx: &mut ModeCtx, unit: Unit, ms: INT| {
                ctx.respawn(&unit, ms)
            })
            .register_fn(
                "add_resource",
                |ctx: &mut ModeCtx, player: INT, name: &str, amount: INT| {
                    ctx.add_resource(player, name, amount)
                },
            );
        View::register_queries::<ModeCtx>(engine, ModeCtx::view);
    }

    /// Registers `ctx.p` and `ctx.state`, the types that read and write the mode's params and
    /// state by name.
    fn register_accessors(engine: &mut Engine) {
        engine
            .register_type_with_name::<ModeParams>("ModeParams")
            .register_indexer_get(
                |params: &mut ModeParams, name: ImmutableString| -> Checked<Dynamic> {
                    params
                        .0
                        .book
                        .schema
                        .param(&name)
                        .ok_or_else(|| ApiError::UnknownParam.fail().into())
                },
            );
        engine
            .register_type_with_name::<ModeStateAccess>("ModeState")
            .register_indexer_get(
                |state: &mut ModeStateAccess, name: ImmutableString| -> Checked<Dynamic> {
                    state.0.state(&name)
                },
            )
            .register_indexer_set(
                |state: &mut ModeStateAccess,
                 name: ImmutableString,
                 value: Dynamic|
                 -> Checked<()> { state.0.set_state(&name, &value) },
            );
    }

    /// `ms` in ticks, rounded up, at least one.
    fn ticks(&self, ms: INT) -> Checked<Ticks> {
        let ms = u64::try_from(ms)
            .ok()
            .ok_or_else(|| ApiError::NegativeTime.fail())?;
        Ok(self
            .book
            .rate
            .ticks(ms)
            .ok_or_else(|| ApiError::TimeTooLarge.fail())?
            .max(Ticks::ONE))
    }

    fn team(&self, name: &str) -> Checked<Team> {
        self.book
            .teams
            .named(name)
            .ok_or_else(|| ApiError::UnknownTeam.fail().into())
    }

    /// Player `player`'s slot, when the session has it.
    fn player(&self, player: INT) -> Checked<PlayerSlot> {
        u32::try_from(player)
            .ok()
            .filter(|&slot| slot < self.book.teams.players())
            .map(PlayerSlot::new)
            .ok_or_else(|| ApiError::UnknownPlayer.fail().into())
    }

    fn state(&self, name: &str) -> Checked<Dynamic> {
        let field = self
            .book
            .schema
            .state_field(name)
            .ok_or_else(|| ApiError::UnknownState.fail())?;
        Ok(self.frame().state[field.index].to_dynamic(&self.view))
    }

    fn set_state(&self, name: &str, value: &Dynamic) -> Checked<()> {
        let field = self
            .book
            .schema
            .state_field(name)
            .ok_or_else(|| ApiError::UnknownState.fail())?;
        let value = StateValue::from_dynamic(field.kind, value)
            .ok_or_else(|| ApiError::WrongStateType.fail())?;
        self.frame().state[field.index] = value;
        Ok(())
    }

    /// Whether `player` may choose the hero `id`: the mode depends on it, and no other player
    /// chose it.
    fn hero_available(&self, player: INT, id: &str) -> Checked<bool> {
        let slot = self.player(player)?;
        let hero = self
            .book
            .roster
            .hero(id)
            .ok_or_else(|| ApiError::UnknownHero.fail())?;
        Ok(!self.frame().picks.taken(slot, hero))
    }

    fn choose_hero(&self, player: INT, id: &str) -> Checked<()> {
        if !self.hero_available(player, id)? {
            return Err(ApiError::HeroTaken.fail().into());
        }
        let slot = self.player(player)?;
        let hero = self
            .book
            .roster
            .hero(id)
            .expect("an available hero is the mode's");
        self.frame().picks.of_mut(slot).hero = Some(hero);
        Ok(())
    }

    /// Chooses `ids`, each a spell the mode depends on, none twice, for `player`.
    fn choose_spells(&self, player: INT, ids: &Array) -> Checked<()> {
        let slot = self.player(player)?;
        let mut frame = self.frame();
        let spells = &mut frame.picks.of_mut(slot).spells;
        spells.clear();
        for id in ids {
            let id = id
                .clone()
                .into_immutable_string()
                .ok()
                .and_then(|id| self.book.roster.spell(&id))
                .ok_or_else(|| ApiError::UnknownSpell.fail())?;
            if spells.contains(&id) {
                return Err(ApiError::RepeatedSpell.fail().into());
            }
            spells.push(id);
        }
        Ok(())
    }

    fn unit_type(&self, name: &str) -> Checked<UnitType> {
        self.view
            .unit_type(name)
            .ok_or_else(|| ApiError::UnknownUnitType.fail().into())
    }

    fn spawn_unit(&self, unit_type: &str, team: &str, pos: Position) -> Checked<()> {
        let effect = ModeEffect::SpawnUnit {
            unit_type: self.unit_type(unit_type)?,
            team: self.team(team)?,
            pos,
        };
        self.frame().effects.push(effect);
        Ok(())
    }

    /// Queues a wave of `types` on `lane` from `team`'s end of it.
    fn spawn_wave(&self, team: &str, lane: &str, types: &Array) -> Checked<()> {
        let team = self.team(team)?;
        self.book.lane_end(team).map_err(ApiError::fail)?;
        let lane = self
            .view
            .lane(lane)
            .ok_or_else(|| ApiError::UnknownLane.fail())?;
        let types = types
            .iter()
            .map(|name| {
                let name = name.clone().into_immutable_string().ok();
                name.and_then(|name| self.view.unit_type(&name))
                    .ok_or_else(|| ApiError::UnknownUnitType.fail().into())
            })
            .collect::<Checked<_>>()?;
        self.frame()
            .effects
            .push(ModeEffect::SpawnWave { team, lane, types });
        Ok(())
    }

    /// Queues a timer `ms` milliseconds from the call, rounded up to whole ticks, at least one.
    fn timer(&self, name: &str, ms: INT, repeat: bool, data: &Dynamic) -> Checked<()> {
        let ticks = self.ticks(ms)?;
        let data = timer_data(data).map_err(ApiError::fail)?;
        self.frame().effects.push(ModeEffect::Timer {
            name: name.to_owned(),
            ticks,
            repeat,
            data,
        });
        Ok(())
    }

    /// Brings back `unit`, which is dead and stays when dead, `ms` after this call, in ticks
    /// rounded up, at least one.
    fn respawn(&self, unit: &Unit, ms: INT) -> Checked<()> {
        let row = unit.row();
        if row.alive {
            return Err(ApiError::RespawnAlive.fail().into());
        }
        if !row.stays {
            return Err(ApiError::RespawnDespawns.fail().into());
        }
        let ticks = self.ticks(ms)?;
        self.frame().effects.push(ModeEffect::Respawn {
            unit: row.id,
            ticks,
        });
        Ok(())
    }

    fn add_resource(&self, player: INT, name: &str, amount: INT) -> Checked<()> {
        let slot = self.player(player)?;
        self.frame()
            .resources
            .add(slot, name, amount)
            .ok_or_else(|| ApiError::ResourceOverflow.fail().into())
    }
}

/// Timer data as state holds it: `None` for `()`, or the value of the first type that takes it.
fn timer_data(data: &Dynamic) -> Result<Option<StateValue>, ApiError> {
    if data.is_unit() {
        return Ok(None);
    }
    [
        StateType::Int,
        StateType::Num,
        StateType::Bool,
        StateType::String,
        StateType::Entity,
        StateType::EntityList,
        StateType::Pos,
        StateType::Vec,
    ]
    .into_iter()
    .find_map(|kind| StateValue::from_dynamic(kind, data))
    .map(Some)
    .ok_or(ApiError::TimerData)
}
