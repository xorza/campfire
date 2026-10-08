use campfire_math::Num;
use campfire_script::rhai::{Array, Dynamic, INT, ImmutableString, NativeCallContext};
use campfire_sim::{Capability, Position, StableId};

use crate::actions::action_slots::ActionSlots;
use crate::actions::actions_column::ActionsColumn;
use crate::combat::combat_column::CombatColumn;
use crate::mode::choice_book::Choice;
use crate::mode::game_map::GameMap;
use crate::mode::group_unit::GroupUnit;
use crate::mode::marker::Marker;
use crate::mode::match_end::MatchResult;
use crate::mode::mode_book::ModeBook;
use crate::mode::mode_call::ModeCall;
use crate::mode::mode_effect::ModeEffect;
use crate::navigation::body_index::IndexedBody;
use crate::navigation::navigation_column::NavigationColumn;
use crate::navigation::path_walker::PathEnd;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::hook::Hook;
use crate::scripts::name_kind::NameKind;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::api_owner::ApiOwner;
use crate::scripts::script_api::data_table::DataTable;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::scripts::script_api::status::Status;
use crate::scripts::state_decl::StateType;
use crate::scripts::state_value::StateValue;
use crate::units::new_unit::NewUnit;
use crate::units::spawn_at::SpawnAt;
use crate::units::team::Team;
use crate::units::unit::Unit;
use crate::units::unit_type::UnitType;
use crate::values::engine_enum::EngineEnum;
use crate::values::rank::Rank;
use crate::values::relation::Relation;

/// The script API of the mode, which every match has: the teams, the map, the avatars and the
/// mode's state, which every role reads, and what only the mode's calls do: the players'
/// choices and resources, spawns, timers, respawns, learning and the match's end.
#[derive(Debug)]
pub(crate) struct ModeApi;

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
            .hook(Hook::CalcDamage, Status::Runs(ApiVersion::FIRST))
            .hook(Hook::CalcHeal, Status::Runs(ApiVersion::FIRST))
            .hook(Hook::OnPlayerJoin, Status::Runs(ApiVersion::FIRST))
            .hook(Hook::OnPlayerLeave, Status::Runs(ApiVersion::FIRST))
            .hook(Hook::OnGenerate, Status::Planned)
            .bind_for(
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
            .data(
                DataTable::Mode,
                &[
                    "script",
                    "combat",
                    "navigation",
                    "slots",
                    "choices",
                    "inputs",
                    "state",
                    "params",
                    "modifiers",
                    "actions",
                    "stats",
                    "pools",
                    "resources",
                    "relations",
                    "tags",
                    "players",
                    "saves",
                ],
                &["state_version"],
            )
            .data(DataTable::Relation, &["teams", "relation", "vision"], &[])
            .data(
                DataTable::Players,
                &["late_join", "bot_takeover", "leaver"],
                &[],
            )
            .data(DataTable::Saves, &["by", "autosave_ms"], &[]);
    }

    /// What every role reads of the mode: the teams, the map, the avatars, the units of a tag,
    /// and, for the mode's calls, the players and its state.
    fn register_reads(api: &mut ApiBuilder<'_>) {
        api.bind(
            MemberSpec::value("teams", "the playing teams' names, the teams with slots"),
            |ctx: &mut Ctx| -> Checked<Array> {
                let teams = &ModeBook::of_or_fail(ctx)?.teams;
                let name = |&team| teams.name(team).expect("a team of the match");
                Ok(teams
                    .playing()
                    .iter()
                    .map(|team| Dynamic::from(ImmutableString::from(name(team))))
                    .collect())
            },
        )
        .bind_for(
            MemberSpec::value(
                "players",
                "how many slots the session has, whatever controls each",
            )
            .roles(RoleSet::MODE),
            |ctx: &mut Ctx| -> Checked<INT> {
                Ok(INT::from(ModeBook::of_or_fail(ctx)?.teams.players()))
            },
        )
        .bind_for(
            MemberSpec::mode_call("team_of", &[&["player"]], "the name of `player`'s team"),
            |ctx: &mut Ctx, player: INT| -> Checked<Dynamic> {
                let book = ModeBook::of_or_fail(ctx)?;
                let slot = book.teams.player(player)?;
                let team = book
                    .teams
                    .of(slot)
                    .expect("a player of the session has a team");
                let name = ctx.view().team_name(team);
                name.map(Dynamic::from)
                    .ok_or_else(|| ApiError::UnknownTeam.fail().into())
            },
        )
        .bind(
            MemberSpec::value("map", "the map: its paths and its markers"),
            |ctx: &mut Ctx| -> Checked<GameMap> { Ok(ModeBook::of_or_fail(ctx)?.map()) },
        )
        .bind_for(
            MemberSpec::value(
                "state",
                "the mode's state fields, by name, to read and write",
            )
            .roles(RoleSet::MODE),
            |ctx: &mut Ctx| Ok(StateAccess(ctx.clone())),
        )
        .bind(
            MemberSpec::call(
                "enemy_team",
                &[&["team"]],
                "the one team that is `team`'s enemy, in a mode of two playing teams",
            )
            .name(0, NameKind::Team),
            |ctx: &mut Ctx, team: &str| -> Checked<Dynamic> {
                let book = ModeBook::of_or_fail(ctx)?;
                let team = ModeApi::team(book, team)?;
                let enemy = book
                    .teams
                    .sole_enemy(team)
                    .ok_or_else(|| ApiError::NoEnemyTeam.fail())?;
                let name = ctx.view().team_name(enemy);
                name.map(Dynamic::from)
                    .ok_or_else(|| ApiError::UnknownTeam.fail().into())
            },
        );
        let avatars = MemberSpec::call(
            "avatars",
            &[&[], &["team"]],
            "the avatars, living or dead, of every team or of `team`, by stable id",
        );
        api.bind(avatars, |ctx: &mut Ctx| ctx.view().avatars(None))
            .bind(avatars, |ctx: &mut Ctx, team: &str| -> Checked<Array> {
                let team = ModeApi::team(ModeBook::of_or_fail(ctx)?, team)?;
                Ok(ctx.view().avatars(Some(team)))
            })
            .bind(
                MemberSpec::call(
                    "units_tagged",
                    &[&["tag"]],
                    "the units of a tag, living or dead, by stable id",
                )
                .name(0, NameKind::Tag),
                |ctx: &mut Ctx, tag: &str| ctx.view().units_tagged(tag),
            );
        api.ty::<StateAccess>("ModeState")
            .index(|_, state: &mut StateAccess, name: &str| ModeApi::state(&state.0, name))
            .index_set(|_, state: &mut StateAccess, name: &str, value: Dynamic| {
                ModeApi::set_state(&state.0, name, &value)
            });
    }

    /// The players' choices, which only the mode's calls make and read.
    fn register_choices(api: &mut ApiBuilder<'_>) {
        api.data(
            DataTable::Choice,
            &["offers", "unique", "count", "slot"],
            &[],
        )
        .data(DataTable::SlotKind, &["name", "ranks", "levels"], &[]);
        let choose = MemberSpec::mode_call(
            "choose",
            &[&["player", "choice", "values"]],
            "records `values`, as many as `choice` takes, each a value it offers, none twice and, \
             in a unique choice, none another player chose, as what `player` chose of it; one \
             value may be given alone",
        )
        .name(1, NameKind::Choice);
        api.bind_for(
            choose,
            |ctx: &mut Ctx, player: INT, choice: ImmutableString, values: Array| {
                ModeApi::choose(ctx, player, &choice, &values)
            },
        )
        .bind_for(
            choose,
            |ctx: &mut Ctx, player: INT, choice: ImmutableString, value: ImmutableString| {
                ModeApi::choose(ctx, player, &choice, &vec![Dynamic::from(value)])
            },
        )
        .bind_for(
            MemberSpec::mode_call(
                "chosen",
                &[&["player", "choice"]],
                "the values `player` chose of `choice`, in order; empty before the player chose",
            )
            .name(1, NameKind::Choice),
            |ctx: &mut Ctx, player: INT, choice: ImmutableString| {
                ModeApi::chosen(ctx, player, &choice)
            },
        )
        .bind_for(
            MemberSpec::mode_call(
                "available",
                &[&["player", "choice", "value"]],
                "whether `player` may choose `value` of `choice`: no other player chose it in a \
                 unique choice",
            )
            .name(1, NameKind::Choice),
            |ctx: &mut Ctx, player: INT, choice: ImmutableString, value: ImmutableString| {
                ModeApi::available(ctx, player, &choice, &value)
            },
        )
        .bind_for(
            MemberSpec::mode_call(
                "offers",
                &[&["choice"]],
                "the values `choice` offers, in order: the avatars in the order of the mode's \
                 dependencies, or the loadout entries by id",
            )
            .name(0, NameKind::Choice),
            |ctx: &mut Ctx, choice: ImmutableString| ModeApi::offers(ctx, &choice),
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
        )
        .name(0, NameKind::UnitType)
        .name(1, NameKind::Team);
        let grant = MemberSpec::mode_call(
            "grant",
            &[&["unit", "kind", "ids"]],
            "puts the actions `ids`, loadout entries the mode depends on, in the slot kind \
             `kind` of `unit`, after its slots of that kind, at the kind's first rank",
        )
        .name(1, NameKind::SlotKind)
        .capability(Capability::Abilities);
        api.bind_for(
            spawn_unit,
            |ctx: &mut Ctx, unit_type: ImmutableString, team: ImmutableString, pos: Position| {
                ModeApi::spawn_unit(ctx, &unit_type, &team, pos, None)
            },
        )
        .bind_for(
            spawn_unit,
            |ctx: &mut Ctx,
             unit_type: ImmutableString,
             team: ImmutableString,
             pos: Position,
             player: INT| {
                ModeApi::spawn_unit(ctx, &unit_type, &team, pos, Some(player))
            },
        )
        .bind_for(
            grant,
            |ctx: &mut Ctx, unit: Unit, kind: ImmutableString, ids: Array| {
                let slots = ActionsColumn::slot_count(ctx.view(), unit.row_index());
                ModeApi::grant(ctx, unit.id, slots, &kind, &ids)
            },
        )
        .bind_for(
            grant,
            |ctx: &mut Ctx, unit: NewUnit, kind: ImmutableString, ids: Array| {
                let book = ModeBook::of_or_fail(ctx)?;
                let slots = book
                    .spawn_slots(unit.unit_type)
                    .map_or(0, |slots| slots.len());
                ModeApi::grant(ctx, unit.id, slots, &kind, &ids)
            },
        )
        .bind_for(
            MemberSpec::mode_call(
                "spawn_group",
                &[&["team", "path", "from", "types"]],
                "spawns `types` of `team` in order at the end `from` of `path`, walking it from \
                 there",
            )
            .name(0, NameKind::Team)
            .name(1, NameKind::Path)
            .takes(2, EngineEnum::PathEnd),
            |ctx: &mut Ctx,
             team: ImmutableString,
             path: ImmutableString,
             from: PathEnd,
             types: Array| { ModeApi::spawn_group(ctx, &team, &path, from, &types) },
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
        api.bind_for(
            MemberSpec::mode_call(
                "timer",
                &[&["name", "ms", "repeat", "data"]],
                "calls `on_timer` `ms` from the call, rounded up to whole ticks, at least one",
            ),
            |ctx: &mut Ctx, name: ImmutableString, ms: INT, repeat: bool, data: Dynamic| {
                ModeApi::timer(ctx, &name, ms, repeat, &data)
            },
        )
        .bind_for(
            MemberSpec::mode_call(
                "respawn",
                &[&["unit", "ms"]],
                "brings back `unit`, dead and of a type that stays, `ms` from the call",
            )
            .capability(Capability::Combat),
            |ctx: &mut Ctx, unit: Unit, ms: INT| ModeApi::respawn(ctx, &unit, ms),
        )
        .bind_for(
            MemberSpec::mode_call(
                "learn",
                &[&["avatar", "slot"]],
                "the ability in `slot` a rank more, up to its last",
            )
            .capability(Capability::Abilities),
            |ctx: &mut Ctx, unit: Unit, slot: INT| ModeApi::learn(ctx, &unit, slot),
        )
        .bind(
            MemberSpec::call(
                "add_resource",
                &[&["player", "name", "amount"]],
                "adds `amount` of the player resource `name`, one the mode declares, to `player`",
            )
            .name(1, NameKind::Resource),
            |ctx: &mut Ctx, player: INT, name: &str, amount: INT| {
                ModeApi::add_resource(ctx, player, name, amount)
            },
        )
        .bind(
            MemberSpec::call(
                "set_relation",
                &[&["a", "b", "relation"]],
                "sets how teams `a` and `b` regard each other, their vision as it was",
            )
            .name(0, NameKind::Team)
            .name(1, NameKind::Team)
            .takes(2, EngineEnum::Relation),
            |ctx: &mut Ctx, a: &str, b: &str, relation: Relation| {
                ModeApi::set_relation(ctx, a, b, relation)
            },
        )
        .bind_for(end, |ctx: &mut Ctx, team: ImmutableString| -> Checked<()> {
            let team = ModeApi::team(ModeBook::of_or_fail(ctx)?, &team)?;
            ModeApi::end(ctx, MatchResult::Won(team))
        })
        .bind_for(end, |ctx: &mut Ctx, (): ()| {
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
            .bind(
                MemberSpec::method(
                    ApiOwner::GameMap,
                    "markers",
                    &[&["tag"]],
                    "the markers with `tag`, in the map's order",
                )
                .name(0, NameKind::MarkerTag),
                |map: GameMap, tag: &str| map.markers(tag),
            );
        api.ty::<Marker>("Marker")
            .bind(field("name", "its name"), |marker: &mut Marker| {
                marker.info().name.clone()
            })
            .bind(
                field("pos", "its point, `()` for a region"),
                |marker: &mut Marker| marker.info().pos.map_or(Dynamic::UNIT, Dynamic::from),
            )
            .bind(
                field("team", "its team's name, `()` with none"),
                |call: NativeCallContext<'_>, marker: &mut Marker| -> Checked<Dynamic> {
                    match marker.info().team {
                        Some(team) => {
                            let name = Ctx::of_call(&call).view().team_name(team);
                            name.map(Dynamic::from)
                                .ok_or_else(|| ApiError::UnknownTeam.fail().into())
                        }
                        None => Ok(Dynamic::UNIT),
                    }
                },
            )
            .bind(
                field("params", "its params, by name"),
                |marker: &mut Marker| marker.info().params.clone(),
            );
    }

    fn team(book: &ModeBook, name: &str) -> Checked<Team> {
        book.teams
            .named(name)
            .ok_or_else(|| ApiError::UnknownTeam.fail().into())
    }

    fn state(ctx: &Ctx, name: &str) -> Checked<Dynamic> {
        let field = ModeBook::of_or_fail(ctx)?
            .schema
            .state_field_named(name)
            .ok_or_else(|| ApiError::UnknownState.fail())?;
        Ok(ModeCall::of(&ctx.frame()).state[field.index].to_dynamic(ctx.view()))
    }

    fn set_state(ctx: &Ctx, name: &str, value: &Dynamic) -> Checked<()> {
        let field = ModeBook::of_or_fail(ctx)?
            .schema
            .state_field_named(name)
            .ok_or_else(|| ApiError::UnknownState.fail())?;
        let value = StateValue::from_dynamic(field.kind, value)
            .ok_or_else(|| ApiError::WrongStateType.fail())?;
        ModeCall::of_mut(&mut *ctx.write()?).state[field.index] = value;
        Ok(())
    }

    /// Ends the match with `result`, once.
    fn end(ctx: &Ctx, result: MatchResult) -> Checked<()> {
        let mut frame = ctx.write()?;
        let call = ModeCall::of_mut(&mut frame);
        if call.ended {
            return Err(ApiError::Ended.fail().into());
        }
        call.ended = true;
        frame.effects.push(ModeEffect::End(result));
        Ok(())
    }

    /// The choice `name` of `book`.
    fn choice<'a>(book: &'a ModeBook, name: &str) -> Checked<&'a Choice> {
        book.choices
            .named(name)
            .ok_or_else(|| ApiError::UnknownChoice.fail().into())
    }

    /// Records `values` as what `player` chose of `choice`.
    fn choose(ctx: &Ctx, player: INT, choice: &str, values: &Array) -> Checked<()> {
        let book = ModeBook::of_or_fail(ctx)?;
        let slot = book.teams.player(player)?;
        let choice = ModeApi::choice(book, choice)?;
        if values.len() != choice.count() {
            return Err(ApiError::ChoiceCount.fail().into());
        }
        let mut offers = Vec::with_capacity(values.len());
        for value in values {
            let offer = value
                .clone()
                .into_immutable_string()
                .ok()
                .and_then(|id| book.roster.offer(choice.offers, &id))
                .ok_or_else(|| ApiError::UnknownChoiceValue.fail())?;
            if offers.contains(&offer) {
                return Err(ApiError::RepeatedChoiceValue.fail().into());
            }
            offers.push(offer);
        }
        let mut frame = ctx.write()?;
        let call = ModeCall::of_mut(&mut frame);
        let taken = |&offer| book.choices.taken(&call.choices, slot, choice, offer);
        if choice.unique && offers.iter().any(taken) {
            return Err(ApiError::ChoiceTaken.fail().into());
        }
        book.choices
            .choose(&mut call.choices, slot, choice, &offers);
        Ok(())
    }

    /// The values `player` chose of `choice`, in order, empty before the player chose.
    fn chosen(ctx: &Ctx, player: INT, choice: &str) -> Checked<Array> {
        let book = ModeBook::of_or_fail(ctx)?;
        let slot = book.teams.player(player)?;
        let choice = ModeApi::choice(book, choice)?;
        let frame = ctx.frame();
        let Some(values) = book
            .choices
            .chosen(&ModeCall::of(&frame).choices, slot, choice)
        else {
            return Ok(Array::new());
        };
        Ok(values
            .map(|offer| Dynamic::from(ImmutableString::from(book.roster.id(choice.offers, offer))))
            .collect())
    }

    /// The values `choice` offers, in order.
    fn offers(ctx: &Ctx, choice: &str) -> Checked<Array> {
        let book = ModeBook::of_or_fail(ctx)?;
        let choice = ModeApi::choice(book, choice)?;
        Ok(book
            .roster
            .ids(choice.offers)
            .map(|id| Dynamic::from(ImmutableString::from(id)))
            .collect())
    }

    /// Whether `player` may choose `value` of `choice`: it offers the value, and no other player
    /// chose it in a unique choice.
    fn available(ctx: &Ctx, player: INT, choice: &str, value: &str) -> Checked<bool> {
        let book = ModeBook::of_or_fail(ctx)?;
        let slot = book.teams.player(player)?;
        let choice = ModeApi::choice(book, choice)?;
        let offer = book
            .roster
            .offer(choice.offers, value)
            .ok_or_else(|| ApiError::UnknownChoiceValue.fail())?;
        let taken = book
            .choices
            .taken(&ModeCall::of(&ctx.frame()).choices, slot, choice, offer);
        Ok(!(choice.unique && taken))
    }

    /// The unit type `name` that `book` can spawn: one with a kit. A projectile or an area type
    /// has none, as only actions deliver its units.
    fn unit_type(ctx: &Ctx, book: &ModeBook, name: &str) -> Checked<UnitType> {
        ctx.view()
            .unit_type_named(name)
            .filter(|&unit_type| book.kit(unit_type).is_some())
            .ok_or_else(|| ApiError::UnknownUnitType.fail().into())
    }

    /// Queues the spawn of a unit of `unit_type` on `team` at `pos`, owned by `player` if it
    /// names one, who plays on `team`: the new unit, with the id the call takes for it.
    fn spawn_unit(
        ctx: &Ctx,
        unit_type: &str,
        team: &str,
        pos: Position,
        player: Option<INT>,
    ) -> Checked<NewUnit> {
        let book = ModeBook::of_or_fail(ctx)?;
        if !ctx.view().bounds().contains(pos) {
            return Err(ApiError::OutOfBounds.fail().into());
        }
        let unit_type = ModeApi::unit_type(ctx, book, unit_type)?;
        let boxed = book
            .kit(unit_type)
            .and_then(|kit| kit.body)
            .filter(|form| form.is_box());
        let body = boxed.map(|form| form.at(Num::ZERO));
        if let Some(body) = body {
            let room = {
                let frame = ctx.frame();
                let spawning = &ModeCall::of(&frame).boxes;
                NavigationColumn::room_for(ctx.view(), pos, body, spawning)
            };
            if !room {
                return Err(ApiError::NoRoom.fail().into());
            }
        }
        let team = ModeApi::team(book, team)?;
        let owner = player.map(|player| book.teams.player(player)).transpose()?;
        if owner.is_some_and(|slot| book.teams.of(slot) != Some(team)) {
            return Err(ApiError::PlayerOffTeam.fail().into());
        }
        let id = ctx.write()?.take_id();
        if let Some(body) = body {
            let spawning = IndexedBody::of(id, pos, &body);
            ModeCall::of_mut(&mut *ctx.write()?).boxes.push(spawning);
        }
        let at = SpawnAt {
            id,
            unit_type,
            team,
            pos,
            angle: Num::ZERO,
        };
        ctx.queue(ModeEffect::SpawnUnit { at, owner })?;
        Ok(NewUnit { id, unit_type })
    }

    /// Queues the actions `ids` into the slot kind `kind` of `unit`, which has `slots` slots
    /// before the grants this call queued.
    fn grant(ctx: &Ctx, unit: StableId, slots: usize, kind: &str, ids: &Array) -> Checked<()> {
        let book = ModeBook::of_or_fail(ctx)?;
        let view = ctx.view();
        let kind = ActionsColumn::kind_named(view, kind)?;
        if ActionsColumn::kind_ranks(view, kind) != book.loadout_ranks {
            return Err(ApiError::SlotKindRanks.fail().into());
        }
        let rank = ActionsColumn::first_rank(view, kind);
        let abilities = ids
            .iter()
            .map(|id| {
                let id = id.clone().into_immutable_string().ok();
                id.and_then(|id| book.roster.loadout_ability(&id))
                    .ok_or_else(|| ApiError::UnknownAction.fail().into())
            })
            .collect::<Checked<Vec<_>>>()?;
        let mut frame = ctx.write()?;
        let queued: usize = frame
            .effects
            .queued::<ModeEffect>()
            .filter_map(|effect| match effect {
                ModeEffect::Grant {
                    unit: granted,
                    abilities,
                    ..
                } if *granted == unit => Some(abilities.len()),
                _ => None,
            })
            .sum();
        if slots + queued + abilities.len() > ActionSlots::LIMIT {
            return Err(ApiError::TooManySlots.fail().into());
        }
        frame.effects.push(ModeEffect::Grant {
            unit,
            kind,
            rank,
            abilities,
        });
        Ok(())
    }

    /// Queues a spawn group of `team`, of `types`, on `path` from its end `from`, each unit with
    /// the id the call takes for it.
    fn spawn_group(ctx: &Ctx, team: &str, path: &str, from: PathEnd, types: &Array) -> Checked<()> {
        let book = ModeBook::of_or_fail(ctx)?;
        let team = ModeApi::team(book, team)?;
        let path = ctx
            .view()
            .path_named(path)
            .ok_or_else(|| ApiError::UnknownPath.fail())?;
        let types = types
            .iter()
            .map(|name| {
                let name = name.clone().into_immutable_string().ok();
                let name = name.ok_or_else(|| ApiError::UnknownUnitType.fail())?;
                ModeApi::unit_type(ctx, book, &name)
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
        frame.effects.push(ModeEffect::SpawnGroup {
            team,
            path,
            from,
            units,
        });
        Ok(())
    }

    /// Queues a timer `ms` milliseconds from the call, rounded up to whole ticks, at least one.
    fn timer(ctx: &Ctx, name: &str, ms: INT, repeat: bool, data: &Dynamic) -> Checked<()> {
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
    fn respawn(ctx: &Ctx, unit: &Unit, ms: INT) -> Checked<()> {
        if unit.read(|row| row.alive) {
            return Err(ApiError::RespawnAlive.fail().into());
        }
        if !CombatColumn::stays(unit) {
            return Err(ApiError::RespawnDespawns.fail().into());
        }
        let ticks = ctx.view().ticks(ms).map_err(ApiError::fail)?;
        ctx.queue(ModeEffect::Respawn {
            unit: unit.id,
            ticks,
        })
    }

    /// Queues a rank more of the ability in `slot` of `unit`: one that has a rank above its
    /// rank, counting the ranks this call queued already.
    fn learn(ctx: &Ctx, unit: &Unit, slot: INT) -> Checked<()> {
        let slot = u8::try_from(slot)
            .ok()
            .ok_or_else(|| ApiError::NoAbilitySlot.fail())?;
        let slot_row = ActionsColumn::slot(ctx.view(), unit.row_index(), slot)
            .ok_or_else(|| ApiError::NoAbilitySlot.fail())?;
        let effect = ModeEffect::Learn {
            unit: unit.id,
            slot,
        };
        let mut frame = ctx.write()?;
        let queued = frame
            .effects
            .queued::<ModeEffect>()
            .filter(|&queued| *queued == effect)
            .count();
        if usize::from(Rank::count(slot_row.rank)) + queued >= usize::from(slot_row.ranks) {
            return Err(ApiError::MaxRank.fail().into());
        }
        frame.effects.push(effect);
        Ok(())
    }

    /// Queues a change of how teams `a` and `b`, two of the mode's, regard each other.
    fn set_relation(ctx: &Ctx, a: &str, b: &str, relation: Relation) -> Checked<()> {
        let book = ModeBook::of_or_fail(ctx)?;
        let (a, b) = (ModeApi::team(book, a)?, ModeApi::team(book, b)?);
        if a == b {
            return Err(ApiError::SelfRelation.fail().into());
        }
        ctx.queue(ModeEffect::SetRelation { a, b, relation })
    }

    fn add_resource(ctx: &Ctx, player: INT, name: &str, amount: INT) -> Checked<()> {
        let book = ModeBook::of_or_fail(ctx)?;
        let slot = book.teams.player(player)?;
        let resource = ctx
            .view()
            .resource_named(name)
            .ok_or_else(|| ApiError::UnknownResource.fail())?;
        let mut frame = ctx.write()?;
        let resources = frame
            .resources_mut()
            .ok_or_else(|| ApiError::NoMode.fail())?;
        resources
            .add(slot, resource, amount)
            .ok_or_else(|| ApiError::ResourceOverflow.fail().into())
    }

    /// Timer data as state holds it: `None` for `()`, or the value of the first type that takes
    /// it.
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
}
