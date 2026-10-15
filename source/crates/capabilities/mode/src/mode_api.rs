use campfire_actions::{ActionSlots, ActionsColumn, SlotKind, SlotKindData};
use campfire_combat::CombatColumn;
use campfire_math::{Num, Vec3};
use campfire_navigation::{IndexedBody, NavigationColumn, PathEnd};
use campfire_players::PlayersCall;
use campfire_script::{
    ApiBuilder, ApiError, ApiOwner, ApiVersion, ArgCtx, CapabilityRefusal, Checked, Ctx, DataTable,
    Hook, MemberSpec, NameKind, RoleSet, ScriptList, ScriptPosition, ScriptText, ScriptValue,
    StateType, StateValue, Status, TextArg, Unit,
};
use campfire_sim::{Capability, StableId};
use campfire_units::{NewUnit, SpawnAt, Units};
use campfire_values::{NameId, PathId, Rank, Relation, ResourceId, Tag, Team, UnitType};

use crate::choice_data::ChoiceData;
use crate::choice_id::ChoiceId;
use crate::error::ModeRefusal;
use crate::game_map::GameMap;
use crate::marker::Marker;
use crate::marker_tag::MarkerTag;
use crate::match_end::MatchResult;
use crate::mode_call::ModeCall;
use crate::mode_effect::ModeEffect;
use crate::mode_spec::ModeSpec;
use crate::players_data::PlayersData;
use crate::relation_data::RelationData;
use crate::saves_data::SavesData;
use crate::spawn_group::{GroupUnit, SpawnGroup};
use crate::team_data::TeamData;

/// The script API of the mode, which every match has: the teams, the map, the avatars and the
/// mode's state, which every role reads, and what only the mode's calls do: the players'
/// choices and resources, spawns, timers, respawns, learning and the match's end.
#[derive(Debug)]
pub struct ModeApi;

/// `ctx.state`: the mode's state fields, by name, to read and write.
#[derive(Debug, Clone)]
pub(crate) struct StateAccess(Ctx);

impl ModeApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        api.engine_enum::<Relation>().engine_enum::<PathEnd>();
        ModeApi::register_map(api);
        ModeApi::register_reads(api);
        ModeApi::register_choices(api);
        ModeApi::register_spawns(api);
        ModeApi::register_changes(api);
        api.hook(Hook::OnMatchStart, Status::Runs(ApiVersion::FIRST))
            .hook(Hook::OnModeInput, Status::Runs(ApiVersion::FIRST))
            .hook(Hook::OnTimer, Status::Runs(ApiVersion::FIRST))
            .hook(Hook::OnUnitDied, Status::Runs(ApiVersion::FIRST))
            .hook(Hook::OnPlayerJoin, Status::Runs(ApiVersion::FIRST))
            .hook(Hook::OnPlayerLeave, Status::Runs(ApiVersion::FIRST))
            .hook(Hook::OnGenerate, Status::Planned)
            .call(
                MemberSpec::mode_call("save", &[&[]], "asks for a save at the end of the tick"),
                |ctx: &mut Ctx| ctx.queue(ModeEffect::Save),
            )
            .plan(
                MemberSpec::value(
                    "carry",
                    "the carry the session loaded, which the mode writes for the next session",
                )
                .roles(RoleSet::MODE),
            )
            .plan(MemberSpec::mode_call(
                "generate",
                &[&["region"]],
                "builds the map's region `region` through `on_generate`",
            ))
            .shared_data(
                DataTable::Mode,
                &[
                    "name",
                    "script",
                    "slots",
                    "choices",
                    "inputs",
                    "state",
                    "params",
                    "modifiers",
                    "actions",
                    "pools",
                    "resources",
                    "teams",
                    "relations",
                    "tags",
                    "players",
                    "saves",
                    "visibility",
                ],
                &["state_version"],
            )
            .data::<TeamData>(&["name"], &[])
            .data::<RelationData>(&["teams", "relation", "vision"], &[])
            .data::<PlayersData>(&["late_join", "bot_takeover", "leaver"], &[])
            .data::<SavesData>(&["by", "autosave_ms"], &[]);
    }

    /// What every role reads of the mode: the teams, the map, the avatars, the units of a tag,
    /// and, for the mode's calls, the players and its state.
    fn register_reads(api: &mut ApiBuilder<'_>) {
        api.call(
            MemberSpec::value("teams", "the playing teams' names, the teams with slots"),
            |ctx: &mut Ctx| -> Checked<ScriptList> {
                let teams = &ModeSpec::of_or_fail(ctx)?.teams;
                let names = ctx.view().names();
                let name = |&team| names.name(team).expect("a team of the match");
                Ok(teams
                    .playing()
                    .iter()
                    .map(name)
                    .map(ScriptValue::of)
                    .collect())
            },
        )
        .call(
            MemberSpec::value(
                "players",
                "how many slots the session has, whatever controls each",
            )
            .roles(RoleSet::MODE),
            |ctx: &mut Ctx| -> Checked<i64> {
                Ok(i64::from(ModeSpec::of_or_fail(ctx)?.teams.players()))
            },
        )
        .call(
            MemberSpec::mode_call("team_of", &[&["player"]], "the name of `player`'s team"),
            |ctx: &mut Ctx, player: i64| -> Checked<ScriptValue> {
                let book = ModeSpec::of_or_fail(ctx)?;
                let slot = book.teams.player(player)?;
                let team = book
                    .teams
                    .of(slot)
                    .expect("a player of the session has a team");
                let name = ctx.view().names().name(team);
                Ok(ScriptValue::of(name.expect("a team of the match")))
            },
        )
        .call(
            MemberSpec::value("map", "the map: its paths and its markers"),
            |ctx: &mut Ctx| -> Checked<GameMap> { Ok(ModeSpec::of_or_fail(ctx)?.map()) },
        )
        .call(
            MemberSpec::value(
                "state",
                "the mode's state fields, by name, to read and write",
            )
            .roles(RoleSet::MODE),
            |ctx: &mut Ctx| Ok(StateAccess(ctx.clone())),
        )
        .call(
            MemberSpec::call(
                "enemy_team",
                &[&["team"]],
                "the one team that is `team`'s enemy, in a mode of two playing teams",
            ),
            |ctx: &mut Ctx, team: Team| -> Checked<ScriptValue> {
                let book = ModeSpec::of_or_fail(ctx)?;
                let enemy = book
                    .teams
                    .sole_enemy(team)
                    .ok_or_else(|| ModeRefusal::NoEnemyTeam.fail())?;
                let name = ctx.view().names().name(enemy);
                Ok(ScriptValue::of(name.expect("a team of the match")))
            },
        );
        let avatars = MemberSpec::call(
            "avatars",
            &[&[], &["team"]],
            "the avatars, living or dead, of every team or of `team`, by stable id",
        );
        api.call(avatars, |ctx: &mut Ctx| {
            Ok(Units::avatars(ctx.view(), None))
        })
        .call(avatars, |ctx: &mut Ctx, team: Team| {
            Ok(Units::avatars(ctx.view(), Some(team)))
        })
        .call(
            MemberSpec::call(
                "units_tagged",
                &[&["tag"]],
                "the units of a tag, living or dead, by stable id",
            ),
            |ctx: &mut Ctx, tag: Tag| Ok(ctx.view().units_tagged(tag)),
        );
        api.ty::<StateAccess>("ModeState")
            .index(|_, state: &mut StateAccess, name: &str| ModeApi::state(&state.0, name))
            .index_set(
                |_, state: &mut StateAccess, name: &str, value: ScriptValue| {
                    ModeApi::set_state(&state.0, name, &value)
                },
            );
    }

    /// The players' choices, which only the mode's calls make and read.
    fn register_choices(api: &mut ApiBuilder<'_>) {
        api.data::<ChoiceData>(&["name", "offers", "unique", "count", "slot"], &[])
            .data::<SlotKindData>(&["name", "ranks", "levels"], &[]);
        let choose = MemberSpec::mode_call(
            "choose",
            &[&["player", "choice", "values"]],
            "records `values`, as many as `choice` takes, each a value it offers, none twice and, \
             in a unique choice, none another player chose, as what `player` chose of it; one \
             value may be given alone",
        );
        api.call(
            choose,
            |ctx: &mut Ctx, player: i64, choice: ChoiceId, values: ScriptList| {
                ModeApi::choose(ctx, player, choice, &values)
            },
        )
        .call(
            choose,
            |ctx: &mut Ctx, player: i64, choice: ChoiceId, value: ScriptText| {
                ModeApi::choose(ctx, player, choice, &[ScriptValue::of(value)])
            },
        )
        .call(
            MemberSpec::mode_call(
                "chosen",
                &[&["player", "choice"]],
                "the values `player` chose of `choice`, in order; empty before the player chose",
            ),
            |ctx: &mut Ctx, player: i64, choice: ChoiceId| ModeApi::chosen(ctx, player, choice),
        )
        .call(
            MemberSpec::mode_call(
                "available",
                &[&["player", "choice", "value"]],
                "whether `player` may choose `value` of `choice`: no other player chose it in a \
                 unique choice",
            ),
            |ctx: &mut Ctx, player: i64, choice: ChoiceId, value: ScriptText| {
                ModeApi::available(ctx, player, choice, &value)
            },
        )
        .call(
            MemberSpec::mode_call(
                "offers",
                &[&["choice"]],
                "the values `choice` offers, in order: the avatars in the order of the mode's \
                 dependencies, or the loadout entries by id",
            ),
            |ctx: &mut Ctx, choice: ChoiceId| ModeApi::offers(ctx, choice),
        );
    }

    /// The spawns only the mode's calls make, and the actions they grant.
    fn register_spawns(api: &mut ApiBuilder<'_>) {
        let spawn_unit = MemberSpec::mode_call(
            "spawn_unit",
            &[&["type", "team", "pos"], &["type", "team", "pos", "player"]],
            "spawns a unit of `type` on `team` at `pos`, within the map's bounds, owned by \
             `player` if given, who plays on `team`, when the call ends; the new unit, for \
             `grant` and its `.state`",
        );
        let grant = MemberSpec::mode_call(
            "grant",
            &[&["unit", "kind", "ids"]],
            "puts the actions `ids`, loadout entries the mode depends on, in the slot kind \
             `kind` of `unit`, after its slots of that kind, at the kind's first rank",
        )
        .capability(Capability::Abilities);
        api.call(
            spawn_unit,
            |ctx: &mut Ctx, unit_type: UnitType, team: Team, pos: ScriptPosition| {
                ModeApi::spawn_unit(ctx, unit_type, team, pos.get(), None)
            },
        )
        .call(
            spawn_unit,
            |ctx: &mut Ctx, unit_type: UnitType, team: Team, pos: ScriptPosition, player: i64| {
                ModeApi::spawn_unit(ctx, unit_type, team, pos.get(), Some(player))
            },
        )
        .call(
            grant,
            |ctx: &mut Ctx, unit: Unit, kind: SlotKind, ids: ScriptList| {
                let slots = ActionsColumn::slot_count(ctx.view(), unit.row_index());
                ModeApi::grant(ctx, unit.id, slots, kind, &ids)
            },
        )
        .call(
            grant,
            |ctx: &mut Ctx, unit: NewUnit, kind: SlotKind, ids: ScriptList| {
                let book = ModeSpec::of_or_fail(ctx)?;
                let slots = book.spawn_slots(unit.unit_type);
                ModeApi::grant(ctx, unit.id, slots, kind, &ids)
            },
        )
        .call(
            MemberSpec::mode_call(
                "spawn_group",
                &[&["team", "path", "from", "types"]],
                "spawns `types` of `team` in order at the end `from` of `path`, walking it from \
                 there",
            ),
            |ctx: &mut Ctx, team: Team, path: PathId, from: PathEnd, types: ScriptList| {
                ModeApi::spawn_group(ctx, team, path, from, &types)
            },
        );
    }

    /// What only the mode's calls do, but adding resources and setting relations, which every
    /// role does: spawns, timers, respawns, learning and the match's end.
    fn register_changes(api: &mut ApiBuilder<'_>) {
        let end = MemberSpec::mode_call(
            "end",
            &[&["team"], &["()"]],
            "ends the match, once: `team` wins, `()` is a draw",
        );
        api.call(
            MemberSpec::mode_call(
                "timer",
                &[&["name", "ms", "repeat", "data"]],
                "calls `on_timer` `ms` from the call, rounded up to whole ticks, at least one",
            ),
            |ctx: &mut Ctx, name: ScriptText, ms: i64, repeat: bool, data: ScriptValue| {
                ModeApi::timer(ctx, &name, ms, repeat, &data)
            },
        )
        .call(
            MemberSpec::mode_call(
                "respawn",
                &[&["unit", "ms"]],
                "brings back `unit`, dead and of a type that stays, `ms` from the call",
            )
            .capability(Capability::Combat),
            |ctx: &mut Ctx, unit: Unit, ms: i64| ModeApi::respawn(ctx, &unit, ms),
        )
        .call(
            MemberSpec::mode_call(
                "learn",
                &[&["avatar", "slot"]],
                "the ability in `slot` a rank more, up to its last",
            )
            .capability(Capability::Abilities),
            |ctx: &mut Ctx, unit: Unit, slot: i64| ModeApi::learn(ctx, &unit, slot),
        )
        .call(
            MemberSpec::call(
                "add_resource",
                &[&["player", "name", "amount"]],
                "adds `amount` of the player resource `name`, one the mode declares, to `player`",
            ),
            |ctx: &mut Ctx, player: i64, resource: ResourceId, amount: i64| {
                ModeApi::add_resource(ctx, player, resource, amount)
            },
        )
        .call(
            MemberSpec::mode_call(
                "set_out",
                &[&["player", "out"]],
                "holds `player` out of play while `out`, as when every avatar it holds is dead",
            ),
            |ctx: &mut Ctx, player: i64, out: bool| ModeApi::set_out(ctx, player, out),
        )
        .call(
            MemberSpec::call(
                "set_relation",
                &[&["a", "b", "relation"]],
                "sets how teams `a` and `b` regard each other, their vision as it was",
            ),
            |ctx: &mut Ctx, a: Team, b: Team, relation: Relation| {
                ModeApi::set_relation(ctx, a, b, relation)
            },
        )
        .call(end, |ctx: &mut Ctx, team: Team| {
            ModeApi::end(ctx, MatchResult::Won(team))
        })
        .call(end, |ctx: &mut Ctx, (): ()| {
            ModeApi::end(ctx, MatchResult::Draw)
        });
    }

    /// `ctx.map`, its paths and its markers.
    fn register_map(api: &mut ApiBuilder<'_>) {
        let field = |name, description| MemberSpec::field(ApiOwner::Marker, name, description);
        api.ty::<GameMap>("Map")
            .bind(
                MemberSpec::field(ApiOwner::GameMap, "paths", "the paths' names")
                    .capability(Capability::Navigation),
                |map: &mut GameMap| map.paths(),
            )
            .method(
                MemberSpec::method(
                    ApiOwner::GameMap,
                    "markers",
                    &[&["tag"]],
                    "the markers with `tag`, in the map's order",
                ),
                |map: GameMap, tag: MarkerTag| Ok(map.markers(tag)),
            );
        api.ty::<Marker>("Marker")
            .bind(field("name", "its name"), |marker: &mut Marker| {
                marker.info().name.clone()
            })
            .bind(
                field("pos", "its point, `()` for a region"),
                |marker: &mut Marker| marker.info().pos.map_or(ScriptValue::UNIT, ScriptValue::of),
            )
            .bind(
                field("team", "its team's name, `()` with none"),
                |ctx: &Ctx, marker: &mut Marker| {
                    let names = ctx.view().names();
                    marker.info().team.map_or(ScriptValue::UNIT, |team| {
                        let name = names.name(team);
                        ScriptValue::of(name.expect("a marker's team is the mode's"))
                    })
                },
            )
            .bind(
                field("params", "its params, by name"),
                |marker: &mut Marker| marker.info().params.clone(),
            );
    }

    fn state(ctx: &Ctx, name: &str) -> Checked<ScriptValue> {
        let field = ModeSpec::of_or_fail(ctx)?
            .schema
            .state
            .field(name)
            .ok_or_else(|| ApiError::UnknownState.fail())?;
        Ok(ModeCall::of(&ctx.frame()).state[field.index()].to_value(ctx.view()))
    }

    fn set_state(ctx: &Ctx, name: &str, value: &ScriptValue) -> Checked<()> {
        let state = &ModeSpec::of_or_fail(ctx)?.schema.state;
        let field = state
            .field(name)
            .ok_or_else(|| ApiError::UnknownState.fail())?;
        let value = StateValue::from_value(state.kind(field), value)
            .ok_or_else(|| ApiError::WrongStateType.fail())?;
        ModeCall::of_mut(&mut *ctx.write()?).state[field.index()] = value;
        Ok(())
    }

    /// Ends the match with `result`, once.
    fn end(ctx: &Ctx, result: MatchResult) -> Checked<()> {
        let mut frame = ctx.write()?;
        let call = ModeCall::of_mut(&mut frame);
        if call.ended {
            return Err(ModeRefusal::Ended.fail().into());
        }
        call.ended = true;
        frame.queue(ModeEffect::End(result));
        Ok(())
    }

    /// Records `values` as what `player` chose of `choice`.
    fn choose(ctx: &Ctx, player: i64, choice: ChoiceId, values: &[ScriptValue]) -> Checked<()> {
        let book = ModeSpec::of_or_fail(ctx)?;
        let slot = book.teams.player(player)?;
        let choice = book.choices.get(choice);
        if values.len() != choice.count() {
            return Err(ModeRefusal::ChoiceCount.fail().into());
        }
        let mut offers = Vec::with_capacity(values.len());
        for value in values {
            let offer = value
                .clone()
                .into_text()
                .and_then(|id| book.roster.offer(choice.offers, id.as_str()))
                .ok_or_else(|| ModeRefusal::UnknownChoiceValue.fail())?;
            if offers.contains(&offer) {
                return Err(ModeRefusal::RepeatedChoiceValue.fail().into());
            }
            offers.push(offer);
        }
        let mut frame = ctx.write()?;
        let call = ModeCall::of_mut(&mut frame);
        let taken = |&offer| call.choices.taken(&book.choices, slot, choice, offer);
        if choice.unique && offers.iter().any(taken) {
            return Err(ModeRefusal::ChoiceTaken.fail().into());
        }
        call.choices.choose(&book.choices, slot, choice, &offers);
        Ok(())
    }

    /// The values `player` chose of `choice`, in order, empty before the player chose.
    fn chosen(ctx: &Ctx, player: i64, choice: ChoiceId) -> Checked<ScriptList> {
        let book = ModeSpec::of_or_fail(ctx)?;
        let slot = book.teams.player(player)?;
        let choice = book.choices.get(choice);
        let frame = ctx.frame();
        let Some(values) = ModeCall::of(&frame)
            .choices
            .chosen(&book.choices, slot, choice)
        else {
            return Ok(ScriptList::default());
        };
        Ok(values
            .map(|offer| ScriptValue::of(ScriptText::from(book.roster.id(choice.offers, offer))))
            .collect())
    }

    /// The values `choice` offers, in order.
    fn offers(ctx: &Ctx, choice: ChoiceId) -> Checked<ScriptList> {
        let book = ModeSpec::of_or_fail(ctx)?;
        let choice = book.choices.get(choice);
        Ok(book
            .roster
            .ids(choice.offers)
            .map(|id| ScriptValue::of(ScriptText::from(id)))
            .collect())
    }

    /// Whether `player` may choose `value` of `choice`: it offers the value, and no other player
    /// chose it in a unique choice.
    fn available(ctx: &Ctx, player: i64, choice: ChoiceId, value: &str) -> Checked<bool> {
        let book = ModeSpec::of_or_fail(ctx)?;
        let slot = book.teams.player(player)?;
        let choice = book.choices.get(choice);
        let offer = book
            .roster
            .offer(choice.offers, value)
            .ok_or_else(|| ModeRefusal::UnknownChoiceValue.fail())?;
        let taken = ModeCall::of(&ctx.frame())
            .choices
            .taken(&book.choices, slot, choice, offer);
        Ok(!(choice.unique && taken))
    }

    /// `unit_type`, when `book` spawns it.
    fn spawnable(book: &ModeSpec, unit_type: UnitType) -> Checked<UnitType> {
        if !book.spawns(unit_type) {
            return Err(ApiError::Unknown(NameKind::UnitType).fail().into());
        }
        Ok(unit_type)
    }

    /// Queues the spawn of a unit of `unit_type` on `team` at `pos`, owned by `player` if it
    /// names one, who plays on `team`: the new unit, with the id the call takes for it.
    fn spawn_unit(
        ctx: &Ctx,
        unit_type: UnitType,
        team: Team,
        pos: Vec3,
        player: Option<i64>,
    ) -> Checked<NewUnit> {
        let book = ModeSpec::of_or_fail(ctx)?;
        if !ctx.view().bounds().contains(pos) {
            return Err(ModeRefusal::OutOfBounds.fail().into());
        }
        let unit_type = ModeApi::spawnable(book, unit_type)?;
        let boxed = book.body(unit_type).filter(|form| form.is_box());
        let body = boxed.map(|form| form.at(Num::ZERO));
        if let Some(body) = body {
            let room = {
                let frame = ctx.frame();
                let spawning = &ModeCall::of(&frame).boxes;
                NavigationColumn::room_for(ctx.view(), pos, body, spawning)
            };
            if !room {
                return Err(ModeRefusal::NoRoom.fail().into());
            }
        }
        let owner = player.map(|player| book.teams.player(player)).transpose()?;
        if owner.is_some_and(|slot| book.teams.of(slot) != Some(team)) {
            return Err(ModeRefusal::PlayerOffTeam.fail().into());
        }
        let id = ctx.write()?.take_id();
        if let Some(body) = body {
            let spawning = IndexedBody::of(id, pos, &body);
            ModeCall::of_mut(&mut *ctx.write()?).boxes.push(spawning);
        }
        let spawn = SpawnAt {
            id,
            unit_type,
            team,
            pos,
            owner,
            angle: Num::ZERO,
        };
        ctx.queue(ModeEffect::SpawnUnit(spawn))?;
        Ok(NewUnit { id, unit_type })
    }

    /// Queues the actions `ids` into the slot kind `kind` of `id`, which has `slots` slots
    /// before the grants this call queued.
    fn grant(
        ctx: &Ctx,
        id: StableId,
        slots: usize,
        kind: SlotKind,
        ids: &[ScriptValue],
    ) -> Checked<()> {
        let book = ModeSpec::of_or_fail(ctx)?;
        let view = ctx.view();
        if ActionsColumn::kind_ranks(view, kind) != book.loadout_ranks {
            return Err(ModeRefusal::SlotKindRanks.fail().into());
        }
        let rank = ActionsColumn::first_rank(view, kind);
        let abilities = ids
            .iter()
            .map(|name| {
                let name = name.clone().into_text();
                name.and_then(|name| book.roster.loadout_ability(name.as_str()))
                    .ok_or_else(|| ApiError::Unknown(NameKind::Ability).fail().into())
            })
            .collect::<Checked<Vec<_>>>()?;
        let mut frame = ctx.write()?;
        let queued: usize = frame
            .queued::<ModeEffect>()
            .filter_map(|effect| match effect {
                ModeEffect::Grant {
                    id: granted,
                    abilities,
                    ..
                } if *granted == id => Some(abilities.len()),
                _ => None,
            })
            .sum();
        if slots + queued + abilities.len() > ActionSlots::LIMIT {
            return Err(ModeRefusal::TooManySlots.fail().into());
        }
        frame.queue(ModeEffect::Grant {
            id,
            kind,
            rank,
            abilities,
        });
        Ok(())
    }

    /// Queues a spawn group of `team`, of `types`, on `path` from its end `from`, each unit with
    /// the id the call takes for it.
    fn spawn_group(
        ctx: &Ctx,
        team: Team,
        path: PathId,
        from: PathEnd,
        types: &[ScriptValue],
    ) -> Checked<()> {
        let book = ModeSpec::of_or_fail(ctx)?;
        let types = types
            .iter()
            .map(|name| {
                let name = name.clone().into_text();
                let name = name.ok_or_else(|| ApiError::Unknown(NameKind::UnitType).fail())?;
                ModeApi::spawnable(book, UnitType::take_text(name, &ArgCtx::Ctx(ctx))?)
            })
            .collect::<Checked<Vec<UnitType>>>()?;
        let mut frame = ctx.write()?;
        let units = types
            .into_iter()
            .map(|unit_type| GroupUnit {
                unit_type,
                id: frame.take_id(),
            })
            .collect();
        frame.queue(ModeEffect::SpawnGroup(SpawnGroup {
            team,
            path,
            from,
            units,
        }));
        Ok(())
    }

    /// Queues a timer `ms` milliseconds from the call, rounded up to whole ticks, at least one.
    fn timer(ctx: &Ctx, name: &str, ms: i64, repeat: bool, data: &ScriptValue) -> Checked<()> {
        let ticks = ctx.view().ticks(ms).map_err(ApiError::fail)?;
        let data = ModeApi::timer_data(data).map_err(ApiError::fail)?;
        ctx.queue(ModeEffect::Timer {
            name: name.to_owned(),
            ticks,
            repeat,
            data,
        })
    }

    /// Brings back `unit`, which is dead and stays when dead, `ms` after this call, in ticks
    /// rounded up, at least one.
    fn respawn(ctx: &Ctx, unit: &Unit, ms: i64) -> Checked<()> {
        if unit.read(|row| row.alive) {
            return Err(ModeRefusal::RespawnAlive.fail().into());
        }
        if !CombatColumn::stays(unit) {
            return Err(ModeRefusal::RespawnDespawns.fail().into());
        }
        let ticks = ctx.view().ticks(ms).map_err(ApiError::fail)?;
        ctx.queue(ModeEffect::Respawn { id: unit.id, ticks })
    }

    /// Queues a rank more of the ability in `slot` of `unit`: one that has a rank above its
    /// rank, counting the ranks this call queued already.
    fn learn(ctx: &Ctx, unit: &Unit, slot: i64) -> Checked<()> {
        let slot = u8::try_from(slot)
            .ok()
            .ok_or_else(|| ModeRefusal::NoAbilitySlot.fail())?;
        let slot_row = ActionsColumn::slot(ctx.view(), unit.row_index(), slot)
            .ok_or_else(|| ModeRefusal::NoAbilitySlot.fail())?;
        let effect = ModeEffect::Learn { id: unit.id, slot };
        let mut frame = ctx.write()?;
        let queued = frame
            .queued::<ModeEffect>()
            .filter(|&queued| *queued == effect)
            .count();
        if usize::from(Rank::count(slot_row.rank)) + queued >= usize::from(slot_row.ranks) {
            return Err(ModeRefusal::MaxRank.fail().into());
        }
        frame.queue(effect);
        Ok(())
    }

    /// Queues a change of how teams `a` and `b`, two of the mode's, regard each other.
    fn set_relation(ctx: &Ctx, a: Team, b: Team, relation: Relation) -> Checked<()> {
        if a == b {
            return Err(ModeRefusal::SelfRelation.fail().into());
        }
        ctx.queue(ModeEffect::SetRelation { a, b, relation })
    }

    /// Queues `player` held out of play, or no more.
    fn set_out(ctx: &Ctx, player: i64, out: bool) -> Checked<()> {
        let book = ModeSpec::of_or_fail(ctx)?;
        let slot = book.teams.player(player)?;
        ctx.queue(ModeEffect::SetOut { slot, out })
    }

    fn add_resource(ctx: &Ctx, player: i64, resource: ResourceId, amount: i64) -> Checked<()> {
        let book = ModeSpec::of_or_fail(ctx)?;
        let slot = book.teams.player(player)?;
        let mut frame = ctx.write()?;
        let players = frame
            .part_mut::<PlayersCall>()
            .expect("every call holds the players'");
        let resources = players
            .resources_mut(slot)
            .expect("a mode that names a resource keeps each player's resources");
        resources
            .add(resource, amount)
            .ok_or_else(|| ModeRefusal::ResourceOverflow.fail().into())
    }

    /// Timer data as state holds it: `None` for `()`, or the value of the first type that takes
    /// it.
    fn timer_data(data: &ScriptValue) -> Result<Option<StateValue>, ApiError> {
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
            StateType::Vec,
        ]
        .into_iter()
        .find_map(|kind| StateValue::from_value(kind, data))
        .map(Some)
        .ok_or_else(|| ModeRefusal::TimerData.refused())
    }
}
