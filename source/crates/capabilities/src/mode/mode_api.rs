use campfire_math::PlayerSlot;
use campfire_script::rhai::{Array, Dynamic, INT, ImmutableString};
use campfire_sim::{Capability, Position};

use crate::mode::game_map::{GameMap, NeutralSpawn};
use crate::mode::match_end::MatchResult;
use crate::mode::mode_book::ModeBook;
use crate::mode::mode_effect::ModeEffect;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::Ctx;
use crate::scripts::effect::Effect;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::hook::Hook;
use crate::scripts::role_set::RoleSet;
use crate::scripts::script_api::{ApiOwner, DataTable, MemberSpec, Status};
use crate::scripts::state_decl::StateType;
use crate::scripts::state_value::StateValue;
use crate::units::team::Team;
use crate::units::unit::Unit;
use crate::units::unit_type::UnitType;

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
        ModeApi::register_changes(api);
        api.hook(Hook::OnMatchStart, "(ctx)", Status::Runs)
            .hook(
                Hook::OnModeInput,
                "(ctx, player, name, value)",
                Status::Runs,
            )
            .hook(Hook::OnTimer, "(ctx, name, data)", Status::Runs)
            .hook(
                Hook::OnUnitDied,
                "(ctx, unit, killer, assisters)",
                Status::Runs,
            )
            .hook(Hook::CalcDamage, "(ctx, d)", Status::Runs)
            .hook(Hook::CalcHeal, "(ctx, h)", Status::Planned)
            .hook(Hook::OnPlayerJoin, "(ctx, player)", Status::Planned)
            .hook(Hook::OnPlayerLeave, "(ctx, player)", Status::Planned)
            .data(
                DataTable::Mode,
                &[
                    "script",
                    "combat",
                    "inputs",
                    "state",
                    "params",
                    "modifiers",
                    "attack_kind",
                    "stats",
                    "resources",
                    "tags",
                ],
                &["state_version"],
            );
    }

    /// What every role reads of the mode: the teams, the map, the avatars, the units of a tag,
    /// and, for the mode's calls, the players and its state.
    fn register_reads(api: &mut ApiBuilder<'_>) {
        api.bind(
            MemberSpec::value("teams", "the playing teams' names"),
            |ctx: &mut Ctx| -> Checked<Array> {
                let teams = ctx.mode_or_fail()?.teams.playing();
                Ok(teams
                    .map(|name| Dynamic::from(ImmutableString::from(name)))
                    .collect())
            },
        )
        .bind(
            MemberSpec::value("players", "how many players the session has").roles(RoleSet::MODE),
            |ctx: &mut Ctx| -> Checked<INT> { Ok(INT::from(ctx.mode_or_fail()?.teams.players())) },
        )
        .bind(
            MemberSpec::value("map", "the map's paths and neutral spawns")
                .capability(Capability::Navigation),
            |ctx: &mut Ctx| -> Checked<GameMap> { Ok(ctx.mode_or_fail()?.map()) },
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
            ),
            |ctx: &mut Ctx, team: &str| -> Checked<Dynamic> {
                let book = ctx.mode_or_fail()?;
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
                let team = ModeApi::team(ctx.mode_or_fail()?, team)?;
                Ok(ctx.view().avatars(Some(team)))
            })
            .bind(
                MemberSpec::call(
                    "units_tagged",
                    "(tag)",
                    "the units of a tag, living or dead, by stable id",
                ),
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

    /// The players' choices, which only the mode's calls make.
    fn register_choices(api: &mut ApiBuilder<'_>) {
        let mode = |name, signature, description| {
            MemberSpec::call(name, signature, description).roles(RoleSet::MODE)
        };
        let available = mode(
            "avatar_available",
            "(player, id)",
            "whether `player` may choose the avatar `id`: the mode depends on it, and no other player chose it",
        );
        let loadout = mode(
            "choose_loadout",
            "(player, ids)",
            "chooses `ids`, each a loadout entry the mode depends on, none twice, for `player`",
        );
        api.bind(available, |ctx: &mut Ctx, player: INT, id: &str| {
            ModeApi::avatar_available(ctx, player, id)
        })
        .bind(
            mode(
                "choose_avatar",
                "(player, id)",
                "chooses the avatar `id` for `player`",
            ),
            |ctx: &mut Ctx, player: INT, id: &str| ModeApi::choose_avatar(ctx, player, id),
        )
        .bind(loadout, |ctx: &mut Ctx, player: INT, ids: Array| {
            ModeApi::choose_loadout(ctx, player, &ids)
        });
    }

    /// What only the mode's calls do, but adding resources, which every role does: spawns,
    /// timers, respawns, learning and the match's end.
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
                "spawn_avatars",
                "()",
                "spawns each chosen avatar not yet spawned, in slot order, at its team's spawn",
            ),
            |ctx: &mut Ctx| -> Checked<()> {
                ctx.require(RoleSet::MODE)?;
                ctx.queue(Effect::Mode(ModeEffect::SpawnAvatars))
            },
        )
        .bind(
            mode(
                "spawn_unit",
                "(type, team, pos)",
                "spawns a unit of `type` on `team` at `pos`, within the map's bounds",
            ),
            |ctx: &mut Ctx, unit_type: &str, team: &str, pos: Position| {
                ModeApi::spawn_unit(ctx, unit_type, team, pos)
            },
        )
        .bind(
            mode(
                "spawn_group",
                "(team, path, types)",
                "spawns `types` in order at `team`'s end of `path`, walking it",
            ),
            |ctx: &mut Ctx, team: &str, path: &str, types: Array| {
                ModeApi::spawn_group(ctx, team, path, &types)
            },
        )
        .bind(
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
                "adds `amount` of the player resource `name` to `player`",
            ),
            |ctx: &mut Ctx, player: INT, name: &str, amount: INT| {
                ModeApi::add_resource(ctx, player, name, amount)
            },
        )
        .bind(end, |ctx: &mut Ctx, team: &str| -> Checked<()> {
            let team = ModeApi::team(ctx.mode_or_fail()?, team)?;
            ModeApi::end(ctx, MatchResult::Won(team))
        })
        .bind(end, |ctx: &mut Ctx, (): ()| {
            ModeApi::end(ctx, MatchResult::Draw)
        });
    }

    /// `ctx.map` and the neutral spawns it lists.
    fn register_map(api: &mut ApiBuilder<'_>) {
        let map = |name, description| {
            MemberSpec::field(ApiOwner::GameMap, name, description)
                .capability(Capability::Navigation)
        };
        let spawn = |name, description| {
            MemberSpec::field(ApiOwner::NeutralSpawn, name, description)
                .capability(Capability::Navigation)
        };
        api.ty::<GameMap>("Map")
            .bind(map("paths", "the paths' names"), |map: &mut GameMap| {
                map.paths.clone()
            })
            .bind(
                map("neutral_spawns", "the neutral spawns"),
                |map: &mut GameMap| map.neutral_spawns.clone(),
            );
        api.ty::<NeutralSpawn>("NeutralSpawn")
            .bind(
                spawn("unit_type", "the unit type it spawns"),
                |spawn: &mut NeutralSpawn| spawn.unit_type.clone(),
            )
            .bind(
                spawn("pos", "where it spawns"),
                |spawn: &mut NeutralSpawn| spawn.pos,
            );
    }

    fn team(book: &ModeBook, name: &str) -> Checked<Team> {
        book.teams
            .named(name)
            .ok_or_else(|| ApiError::UnknownTeam.fail().into())
    }

    /// Player `player`'s slot, when the session has it.
    fn player(book: &ModeBook, player: INT) -> Checked<PlayerSlot> {
        u32::try_from(player)
            .ok()
            .filter(|&slot| slot < book.teams.players())
            .map(PlayerSlot::new)
            .ok_or_else(|| ApiError::UnknownPlayer.fail().into())
    }

    fn state(ctx: &Ctx, name: &str) -> Checked<Dynamic> {
        ctx.require(RoleSet::MODE)?;
        let field = ctx
            .mode_or_fail()?
            .schema
            .state_field(name)
            .ok_or_else(|| ApiError::UnknownState.fail())?;
        Ok(ctx.frame().state[field.index].to_dynamic(ctx.view()))
    }

    fn set_state(ctx: &Ctx, name: &str, value: &Dynamic) -> Checked<()> {
        ctx.require(RoleSet::MODE)?;
        let field = ctx
            .mode_or_fail()?
            .schema
            .state_field(name)
            .ok_or_else(|| ApiError::UnknownState.fail())?;
        let value = StateValue::from_dynamic(field.kind, value)
            .ok_or_else(|| ApiError::WrongStateType.fail())?;
        ctx.write()?.state[field.index] = value;
        Ok(())
    }

    /// Ends the match with `result`, once.
    fn end(ctx: &Ctx, result: MatchResult) -> Checked<()> {
        ctx.require(RoleSet::MODE)?;
        let mut frame = ctx.write()?;
        if frame.ended {
            return Err(ApiError::Ended.fail().into());
        }
        frame.ended = true;
        frame.effects.push(Effect::Mode(ModeEffect::End(result)));
        Ok(())
    }

    /// Whether `player` may choose the avatar `id`: the mode depends on it, and no other player
    /// chose it.
    fn avatar_available(ctx: &Ctx, player: INT, id: &str) -> Checked<bool> {
        ctx.require(RoleSet::MODE)?;
        let book = ctx.mode_or_fail()?;
        let slot = ModeApi::player(book, player)?;
        let avatar = book
            .roster
            .avatar(id)
            .ok_or_else(|| ApiError::UnknownAvatar.fail())?;
        Ok(!ctx.frame().picks.taken(slot, avatar))
    }

    fn choose_avatar(ctx: &Ctx, player: INT, id: &str) -> Checked<()> {
        if !ModeApi::avatar_available(ctx, player, id)? {
            return Err(ApiError::AvatarTaken.fail().into());
        }
        let book = ctx.mode_or_fail()?;
        let slot = ModeApi::player(book, player)?;
        let avatar = book
            .roster
            .avatar(id)
            .expect("an available avatar is the mode's");
        ctx.write()?.picks.of_mut(slot).avatar = Some(avatar);
        Ok(())
    }

    /// Chooses `ids`, each a loadout entry the mode depends on, none twice, for `player`.
    fn choose_loadout(ctx: &Ctx, player: INT, ids: &Array) -> Checked<()> {
        ctx.require(RoleSet::MODE)?;
        let book = ctx.mode_or_fail()?;
        let slot = ModeApi::player(book, player)?;
        let mut frame = ctx.write()?;
        let loadout = &mut frame.picks.of_mut(slot).loadout;
        loadout.clear();
        for id in ids {
            let id = id
                .clone()
                .into_immutable_string()
                .ok()
                .and_then(|id| book.roster.loadout(&id))
                .ok_or_else(|| ApiError::UnknownLoadout.fail())?;
            if loadout.contains(&id) {
                return Err(ApiError::RepeatedLoadout.fail().into());
            }
            loadout.push(id);
        }
        Ok(())
    }

    fn unit_type(ctx: &Ctx, name: &str) -> Checked<UnitType> {
        ctx.view()
            .unit_type(name)
            .ok_or_else(|| ApiError::UnknownUnitType.fail().into())
    }

    /// Queues a unit of `unit_type` on `team` at `pos`, which is within the map's bounds.
    fn spawn_unit(ctx: &Ctx, unit_type: &str, team: &str, pos: Position) -> Checked<()> {
        ctx.require(RoleSet::MODE)?;
        let book = ctx.mode_or_fail()?;
        if !book.bounds.contains(pos) {
            return Err(ApiError::OutOfBounds.fail().into());
        }
        let effect = ModeEffect::SpawnUnit {
            unit_type: ModeApi::unit_type(ctx, unit_type)?,
            team: ModeApi::team(book, team)?,
            pos,
        };
        ctx.queue(Effect::Mode(effect))
    }

    /// Queues a spawn group of `types` on `path` from `team`'s end of it.
    fn spawn_group(ctx: &Ctx, team: &str, path: &str, types: &Array) -> Checked<()> {
        ctx.require(RoleSet::MODE)?;
        let book = ctx.mode_or_fail()?;
        let team = ModeApi::team(book, team)?;
        book.path_end(team).map_err(ApiError::fail)?;
        let path = ctx
            .view()
            .path(path)
            .ok_or_else(|| ApiError::UnknownPath.fail())?;
        let types = types
            .iter()
            .map(|name| {
                let name = name.clone().into_immutable_string().ok();
                name.and_then(|name| ctx.view().unit_type(&name))
                    .ok_or_else(|| ApiError::UnknownUnitType.fail().into())
            })
            .collect::<Checked<_>>()?;
        ctx.queue(Effect::Mode(ModeEffect::SpawnGroup { team, path, types }))
    }

    /// Queues a timer `ms` milliseconds from the call, rounded up to whole ticks, at least one.
    fn timer(ctx: &Ctx, name: &str, ms: INT, repeat: bool, data: &Dynamic) -> Checked<()> {
        ctx.require(RoleSet::MODE)?;
        let ticks = ctx.view().ticks(ms)?;
        let data = ModeApi::timer_data(data).map_err(ApiError::fail)?;
        ctx.queue(Effect::Mode(ModeEffect::Timer {
            name: name.to_owned(),
            ticks,
            repeat,
            data,
        }))
    }

    /// Brings back `unit`, which is dead and stays when dead, `ms` after this call, in ticks
    /// rounded up, at least one.
    fn respawn(ctx: &Ctx, unit: &Unit, ms: INT) -> Checked<()> {
        ctx.require(RoleSet::MODE)?;
        let row = unit.row();
        if row.alive {
            return Err(ApiError::RespawnAlive.fail().into());
        }
        if !row.stays {
            return Err(ApiError::RespawnDespawns.fail().into());
        }
        let ticks = ctx.view().ticks(ms)?;
        ctx.queue(Effect::Mode(ModeEffect::Respawn {
            unit: row.id,
            ticks,
        }))
    }

    /// Queues a rank more of the ability in `slot` of `unit`: one that has a rank above its
    /// rank, counting the ranks this call queued already.
    fn learn(ctx: &Ctx, unit: &Unit, slot: INT) -> Checked<()> {
        ctx.require(RoleSet::MODE)?;
        let row = unit.row();
        let slot = u8::try_from(slot)
            .ok()
            .ok_or_else(|| ApiError::NoAbilitySlot.fail())?;
        let slot_row = ctx
            .view()
            .slot(&row, slot)
            .ok_or_else(|| ApiError::NoAbilitySlot.fail())?;
        let effect = Effect::Mode(ModeEffect::Learn { unit: row.id, slot });
        let mut frame = ctx.write()?;
        let queued = frame
            .effects
            .iter()
            .filter(|&queued| *queued == effect)
            .count();
        if usize::from(slot_row.rank) + queued >= usize::from(slot_row.ranks) {
            return Err(ApiError::MaxRank.fail().into());
        }
        frame.effects.push(effect);
        Ok(())
    }

    fn add_resource(ctx: &Ctx, player: INT, name: &str, amount: INT) -> Checked<()> {
        let slot = ModeApi::player(ctx.mode_or_fail()?, player)?;
        let mut frame = ctx.write()?;
        let resources = frame
            .resources_mut()
            .ok_or_else(|| ApiError::NoMode.fail())?;
        resources
            .add(slot, name, amount)
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
