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
use crate::mode::new_unit::NewUnit;
use crate::navigation::path_walker::PathEnd;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::api_version::ApiVersion;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::hook::Hook;
use crate::scripts::name_kind::NameKind;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::{ApiOwner, DataTable, MemberSpec, Status};
use crate::scripts::state_decl::StateType;
use crate::scripts::state_value::StateValue;
use crate::units::spawner::SpawnAt;
use crate::units::team::Team;
use crate::units::unit::Unit;
use crate::units::unit_type::UnitType;
use crate::values::attitude::Attitude;

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
            .hook(Hook::OnPlayerJoin, Status::Planned)
            .hook(Hook::OnPlayerLeave, Status::Planned)
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
                ],
                &["state_version"],
            )
            .data(DataTable::Relation, &["teams", "relation", "vision"], &[]);
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
        .bind(
            MemberSpec::value("players", "how many players the session has").roles(RoleSet::MODE),
            |ctx: &mut Ctx| -> Checked<INT> {
                Ok(INT::from(ModeBook::of_or_fail(ctx)?.teams.players()))
            },
        )
        .bind(
            MemberSpec::call("team_of", "(player)", "the name of `player`'s team")
                .roles(RoleSet::MODE),
            |ctx: &mut Ctx, player: INT| -> Checked<Dynamic> {
                let book = ModeBook::of_or_fail(ctx)?;
                let slot = book.teams.player(player)?;
                let team = book
                    .teams
                    .of(slot)
                    .expect("a player of the session has a team");
                ctx.view().team_name(team)
            },
        )
        .bind(
            MemberSpec::value("map", "the map: its paths and its markers"),
            |ctx: &mut Ctx| -> Checked<GameMap> { Ok(ModeBook::of_or_fail(ctx)?.map()) },
        )
        .bind(
            MemberSpec::value(
                "state",
                "the mode's state fields, by name, to read and write",
            )
            .roles(RoleSet::MODE),
            |ctx: &mut Ctx| StateAccess(ctx.clone()),
        )
        .bind(
            MemberSpec::call(
                "enemy_team",
                "(team)",
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
                ctx.view().team_name(enemy)
            },
        );
        let avatars = MemberSpec::call(
            "avatars",
            "() or (team)",
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
                    "(tag)",
                    "the units of a tag, living or dead, by stable id",
                )
                .name(0, NameKind::Tag),
                |ctx: &mut Ctx, tag: &str| ctx.view().units_tagged(tag),
            );
        api.ty::<StateAccess>("ModeState")
            .index(
                |state: &mut StateAccess, name: ImmutableString| -> Checked<Dynamic> {
                    ModeApi::state(&state.0, &name)
                },
            )
            .index_set(
                |state: &mut StateAccess, name: ImmutableString, value: Dynamic| -> Checked<()> {
                    ModeApi::set_state(&state.0, &name, &value)
                },
            );
    }

    /// The players' choices, which only the mode's calls make and read.
    fn register_choices(api: &mut ApiBuilder<'_>) {
        api.data(
            DataTable::Choice,
            &["offers", "unique", "count", "slot"],
            &[],
        )
        .data(DataTable::SlotKind, &["name", "ranks"], &["levels"]);
        let mode = |name, signature, description| {
            MemberSpec::call(name, signature, description).roles(RoleSet::MODE)
        };
        let choose = mode(
            "choose",
            "(player, choice, values)",
            "records `values`, as many as `choice` takes, each a value it offers, none twice and, \
             in a unique choice, none another player chose, as what `player` chose of it; one \
             value may be given alone",
        )
        .name(1, NameKind::Choice);
        api.bind(
            choose,
            |ctx: &mut Ctx, player: INT, choice: &str, values: Array| {
                ModeApi::choose(ctx, player, choice, &values)
            },
        )
        .bind(
            choose,
            |ctx: &mut Ctx, player: INT, choice: &str, value: ImmutableString| {
                ModeApi::choose(ctx, player, choice, &vec![Dynamic::from(value)])
            },
        )
        .bind(
            mode(
                "chosen",
                "(player, choice)",
                "the values `player` chose of `choice`, in order; empty before the player chose",
            )
            .name(1, NameKind::Choice),
            |ctx: &mut Ctx, player: INT, choice: &str| ModeApi::chosen(ctx, player, choice),
        )
        .bind(
            mode(
                "available",
                "(player, choice, value)",
                "whether `player` may choose `value` of `choice`: no other player chose it in a \
                 unique choice",
            )
            .name(1, NameKind::Choice),
            |ctx: &mut Ctx, player: INT, choice: &str, value: &str| {
                ModeApi::available(ctx, player, choice, value)
            },
        );
    }

    /// The spawns only the mode's calls make, and the actions they grant.
    fn register_spawns(api: &mut ApiBuilder<'_>) {
        let mode = |name, signature, description| {
            MemberSpec::call(name, signature, description).roles(RoleSet::MODE)
        };
        let spawn_unit = mode(
            "spawn_unit",
            "(type, team, pos) or (type, team, pos, player)",
            "spawns a unit of `type` on `team` at `pos`, within the map's bounds, owned by \
             `player` if given, when the call ends; the new unit, for `grant`",
        )
        .name(0, NameKind::UnitType)
        .name(1, NameKind::Team);
        let grant = mode(
            "grant",
            "(unit, kind, ids)",
            "puts the actions `ids`, loadout entries the mode depends on, in the slot kind \
             `kind` of `unit`, after its slots of that kind, at the kind's first rank",
        )
        .name(1, NameKind::SlotKind)
        .capability(Capability::Abilities);
        api.ty::<NewUnit>("NewUnit");
        api.bind(
            spawn_unit,
            |ctx: &mut Ctx, unit_type: &str, team: &str, pos: Position| {
                ModeApi::spawn_unit(ctx, unit_type, team, pos, None)
            },
        )
        .bind(
            spawn_unit,
            |ctx: &mut Ctx, unit_type: &str, team: &str, pos: Position, player: INT| {
                ModeApi::spawn_unit(ctx, unit_type, team, pos, Some(player))
            },
        )
        .bind(
            grant,
            |ctx: &mut Ctx, unit: Unit, kind: &str, ids: Array| {
                let slots = ActionsColumn::slot_count(ctx.view(), unit.row_index());
                ModeApi::grant(ctx, unit.id, slots, kind, &ids)
            },
        )
        .bind(
            grant,
            |ctx: &mut Ctx, unit: NewUnit, kind: &str, ids: Array| {
                let book = ModeBook::of_or_fail(ctx)?;
                let slots = book.actions(unit.unit_type).len();
                ModeApi::grant(ctx, unit.id, slots, kind, &ids)
            },
        )
        .bind(
            mode(
                "spawn_group",
                "(team, path, from, types)",
                "spawns `types` of `team` in order at the end `from`, `start` or `end`, of `path`, \
                 walking it from there",
            )
            .name(0, NameKind::Team)
            .name(1, NameKind::Path),
            |ctx: &mut Ctx, team: &str, path: &str, from: &str, types: Array| {
                ModeApi::spawn_group(ctx, team, path, from, &types)
            },
        );
    }

    /// What only the mode's calls do, but adding resources and setting relations, which every
    /// role does: spawns, timers, respawns, learning and the match's end.
    fn register_changes(api: &mut ApiBuilder<'_>) {
        let mode = |name, signature, description| {
            MemberSpec::call(name, signature, description).roles(RoleSet::MODE)
        };
        let end = mode(
            "end",
            "(team) or (())",
            "ends the match, once: `team` wins, `()` is a draw",
        );
        api.bind(
            mode(
                "timer",
                "(name, ms, repeat, data)",
                "calls `on_timer` `ms` from the call, rounded up to whole ticks, at least one",
            ),
            |ctx: &mut Ctx, name: &str, ms: INT, repeat: bool, data: Dynamic| {
                ModeApi::timer(ctx, name, ms, repeat, &data)
            },
        )
        .bind(
            mode(
                "respawn",
                "(unit, ms)",
                "brings back `unit`, dead and of a type that stays, `ms` from the call",
            )
            .capability(Capability::Combat),
            |ctx: &mut Ctx, unit: Unit, ms: INT| ModeApi::respawn(ctx, &unit, ms),
        )
        .bind(
            mode(
                "learn",
                "(avatar, slot)",
                "the ability in `slot` a rank more, up to its last",
            )
            .capability(Capability::Abilities),
            |ctx: &mut Ctx, unit: Unit, slot: INT| ModeApi::learn(ctx, &unit, slot),
        )
        .bind(
            MemberSpec::call(
                "add_resource",
                "(player, name, amount)",
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
                "(a, b, relation)",
                "sets how teams `a` and `b` regard each other, `hostile`, `neutral` or `friendly`, \
                 their vision as it was",
            )
            .name(0, NameKind::Team)
            .name(1, NameKind::Team),
            |ctx: &mut Ctx, a: &str, b: &str, relation: &str| {
                ModeApi::set_relation(ctx, a, b, relation)
            },
        )
        .bind(end, |ctx: &mut Ctx, team: &str| -> Checked<()> {
            let team = ModeApi::team(ModeBook::of_or_fail(ctx)?, team)?;
            ModeApi::end(ctx, MatchResult::Won(team))
        })
        .bind(end, |ctx: &mut Ctx, (): ()| {
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
                    "(tag)",
                    "the markers with `tag`, in the map's order",
                )
                .name(0, NameKind::MarkerTag),
                |map: &mut GameMap, tag: &str| map.markers(tag),
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
                |call: NativeCallContext<'_>, marker: &mut Marker| match marker.info().team {
                    Some(team) => Ctx::of_call(&call).view().team_name(team),
                    None => Ok(Dynamic::UNIT),
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
        ctx.require(RoleSet::MODE)?;
        let field = ModeBook::of_or_fail(ctx)?
            .schema
            .state_field_named(name)
            .ok_or_else(|| ApiError::UnknownState.fail())?;
        Ok(ModeCall::of(&ctx.frame()).state[field.index].to_dynamic(ctx.view()))
    }

    fn set_state(ctx: &Ctx, name: &str, value: &Dynamic) -> Checked<()> {
        ctx.require(RoleSet::MODE)?;
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
        ctx.require(RoleSet::MODE)?;
        let mut frame = ctx.write()?;
        let call = ModeCall::of_mut(&mut frame);
        if call.ended {
            return Err(ApiError::Ended.fail().into());
        }
        call.ended = true;
        frame.effects.push(ModeEffect::End(result));
        Ok(())
    }

    /// The choice `name` of the mode of `ctx`.
    fn choice<'a>(ctx: &'a Ctx, name: &str) -> Checked<&'a Choice> {
        let book = ModeBook::of_or_fail(ctx)?;
        book.choices
            .named(name)
            .ok_or_else(|| ApiError::UnknownChoice.fail().into())
    }

    /// Records `values` as what `player` chose of `choice`.
    fn choose(ctx: &Ctx, player: INT, choice: &str, values: &Array) -> Checked<()> {
        ctx.require(RoleSet::MODE)?;
        let book = ModeBook::of_or_fail(ctx)?;
        let slot = book.teams.player(player)?;
        let choice = ModeApi::choice(ctx, choice)?;
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
        ctx.require(RoleSet::MODE)?;
        let book = ModeBook::of_or_fail(ctx)?;
        let slot = book.teams.player(player)?;
        let choice = ModeApi::choice(ctx, choice)?;
        let frame = ctx.frame();
        let Some(values) = book
            .choices
            .chosen(&ModeCall::of(&frame).choices, slot, choice)
        else {
            return Ok(Array::new());
        };
        Ok(values
            .iter()
            .map(|offer| {
                let offer = offer.expect("a chosen choice has every value");
                Dynamic::from(ImmutableString::from(book.roster.id(choice.offers, offer)))
            })
            .collect())
    }

    /// Whether `player` may choose `value` of `choice`: it offers the value, and no other player
    /// chose it in a unique choice.
    fn available(ctx: &Ctx, player: INT, choice: &str, value: &str) -> Checked<bool> {
        ctx.require(RoleSet::MODE)?;
        let book = ModeBook::of_or_fail(ctx)?;
        let slot = book.teams.player(player)?;
        let choice = ModeApi::choice(ctx, choice)?;
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
    /// names one: the new unit, with the id the call takes for it.
    fn spawn_unit(
        ctx: &Ctx,
        unit_type: &str,
        team: &str,
        pos: Position,
        player: Option<INT>,
    ) -> Checked<NewUnit> {
        ctx.require(RoleSet::MODE)?;
        let book = ModeBook::of_or_fail(ctx)?;
        if !ctx.view().bounds().contains(pos) {
            return Err(ApiError::OutOfBounds.fail().into());
        }
        let unit_type = ModeApi::unit_type(ctx, book, unit_type)?;
        let team = ModeApi::team(book, team)?;
        let owner = player.map(|player| book.teams.player(player)).transpose()?;
        let id = ctx.write()?.ids.allocate();
        let at = SpawnAt {
            id,
            unit_type,
            team,
            pos,
        };
        ctx.queue(ModeEffect::SpawnUnit { at, owner })?;
        Ok(NewUnit { id, unit_type })
    }

    /// Queues the actions `ids` into the slot kind `kind` of `unit`, which has `slots` slots
    /// before the grants this call queued.
    fn grant(ctx: &Ctx, unit: StableId, slots: usize, kind: &str, ids: &Array) -> Checked<()> {
        ctx.require(RoleSet::MODE)?;
        let book = ModeBook::of_or_fail(ctx)?;
        let kind = book
            .slot_kinds
            .named(kind)
            .ok_or_else(|| ApiError::UnknownSlotKind.fail())?;
        if book.slot_kinds.ranks(kind) != book.loadout_ranks {
            return Err(ApiError::SlotKindRanks.fail().into());
        }
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
            abilities,
        });
        Ok(())
    }

    /// Queues a spawn group of `team`, of `types`, on `path` from its end `from`, each unit with
    /// the id the call takes for it.
    fn spawn_group(ctx: &Ctx, team: &str, path: &str, from: &str, types: &Array) -> Checked<()> {
        ctx.require(RoleSet::MODE)?;
        let book = ModeBook::of_or_fail(ctx)?;
        let team = ModeApi::team(book, team)?;
        let from = PathEnd::named(from).ok_or_else(|| ApiError::UnknownPathEnd.fail())?;
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
                id: frame.ids.allocate(),
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
        ctx.require(RoleSet::MODE)?;
        let ticks = ctx.view().ticks(ms)?;
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
        ctx.require(RoleSet::MODE)?;
        let row = unit.row();
        if row.alive {
            return Err(ApiError::RespawnAlive.fail().into());
        }
        if !CombatColumn::stays(unit) {
            return Err(ApiError::RespawnDespawns.fail().into());
        }
        let ticks = ctx.view().ticks(ms)?;
        ctx.queue(ModeEffect::Respawn {
            unit: row.id,
            ticks,
        })
    }

    /// Queues a rank more of the ability in `slot` of `unit`: one that has a rank above its
    /// rank, counting the ranks this call queued already.
    fn learn(ctx: &Ctx, unit: &Unit, slot: INT) -> Checked<()> {
        ctx.require(RoleSet::MODE)?;
        let row = unit.row();
        let slot = u8::try_from(slot)
            .ok()
            .ok_or_else(|| ApiError::NoAbilitySlot.fail())?;
        let slot_row = ActionsColumn::slot(ctx.view(), unit.row_index(), slot)
            .ok_or_else(|| ApiError::NoAbilitySlot.fail())?;
        let effect = ModeEffect::Learn { unit: row.id, slot };
        let mut frame = ctx.write()?;
        let queued = frame
            .effects
            .queued::<ModeEffect>()
            .filter(|&queued| *queued == effect)
            .count();
        if usize::from(slot_row.rank) + queued >= usize::from(slot_row.ranks) {
            return Err(ApiError::MaxRank.fail().into());
        }
        frame.effects.push(effect);
        Ok(())
    }

    /// Queues a change of how teams `a` and `b`, two of the mode's, regard each other.
    fn set_relation(ctx: &Ctx, a: &str, b: &str, relation: &str) -> Checked<()> {
        let book = ModeBook::of_or_fail(ctx)?;
        let (a, b) = (ModeApi::team(book, a)?, ModeApi::team(book, b)?);
        let attitude = Attitude::named(relation).ok_or_else(|| ApiError::UnknownRelation.fail())?;
        if a == b {
            return Err(ApiError::SelfRelation.fail().into());
        }
        ctx.queue(ModeEffect::SetRelation { a, b, attitude })
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
