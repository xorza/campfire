use campfire_actions::{ActionSlots, ActionsColumn, SlotKind, SlotKindData};
use campfire_common::PlayerSlot;
use campfire_geometry::GridSearch;
use campfire_math::{Num, Vec3};
use campfire_navigation::{IndexedBody, NavigationColumn, PathEnd, RoomAsk, RoomSearch};
use campfire_players::{PlayerResources, PlayerSlotHolder, PlayersCall};
use campfire_script::{
    ApiBuilder, ApiError, ApiHolder, ApiVersion, ArgCtx, CapabilityRefusal, Checked, Ctx,
    DataTable, Frame, IdName, IdNames, ListPlace, Listing, MaybeName, MemberDecl,
    PlannedCapability, PlayerHandle, RoleSet, ScriptHandle, ScriptList, ScriptPosition, ScriptText,
    ScriptTicks, ScriptValue, StateField, StateType, StateValue, Status, TextArg, Unit, UnitAt,
};
use campfire_sim::{Capability, StableId};
use campfire_units::{Body, SpawnAt, Units};
use campfire_values::{ActionId, NameId, Named, PathId, Relation, ResourceId, Tag, Team, UnitType};
use campfire_visibility::DataKind;

use crate::bounty_data::{BountyData, ModeBountyData};
use crate::choice_data::ChoiceData;
use crate::choice_id::ChoiceId;
use crate::error::ModeRefusal;
use crate::game_map::GameMap;
use crate::marker::Marker;
use crate::marker_tag::MarkerTag;
use crate::match_end::MatchResult;
use crate::mode_call::ModeCall;
use crate::mode_ctx::ModeCtx;
use crate::mode_data::ModeData;
use crate::mode_effect::{ModeEffect, SlotGrant};
use crate::mode_spec::ModeSpec;
use crate::option_data::OptionData;
use crate::player_state_access::PlayerStateAccess;
use crate::players_data::PlayersData;
use crate::relation_data::RelationData;
use crate::result_handle::ResultHandle;
use crate::saves_data::SavesData;
use crate::spawn_group::{GroupUnit, SpawnGroup};
use crate::team_data::TeamData;
use crate::timer_id::TimerId;
use crate::timers::TimerStart;
use crate::victory_data::VictoryData;

/// The script API of the mode, which every match has: the teams, the map, the avatars and the
/// mode's state, which every role reads, and what only the mode's calls do: the players'
/// choices and resources, spawns, grants, timers and the match's end.
#[derive(Debug)]
pub(crate) struct ModeApi;

/// Whether a timer fires once, or again each period after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Repeat {
    Once,
    Every,
}

/// `ctx.state`: the mode's state fields, by name, to read and write.
#[derive(Debug, Clone)]
pub(crate) struct StateAccess(Ctx);

impl ModeApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        api.engine_enum::<Relation>().engine_enum::<PathEnd>();
        Self::register_map(api);
        Self::register_reads(api);
        Self::register_choices(api);
        Self::register_spawns(api);
        Self::register_changes(api);
        Self::register_player(api);
        PlayerStateAccess::register(api);
        ResultHandle::register(api);
        api.call(
            MemberDecl::mode_call("save", &[&[]], "asks for a save at the end of the tick"),
            |ctx: &mut Ctx| ctx.queue(ModeEffect::Save),
        )
        .plan(
            MemberDecl::value(
                "carry",
                "the carry the session loaded, which the mode writes for the next session",
            )
            .roles(RoleSet::MODE),
        )
        .plan(MemberDecl::mode_call(
            "generate",
            &[&["region"]],
            "builds the map's region `region` through `on_generate`",
        ))
        .plan(
            MemberDecl::call(
                "quest_stage",
                &[&["player", "id", "stage"]],
                "sets the stage of `player`'s quest `id`",
            )
            .planned_in(PlannedCapability::Quests),
        )
        .plan(
            MemberDecl::call(
                "quest",
                &[&["player", "id"]],
                "the stage of `player`'s quest `id`, `()` before it starts",
            )
            .planned_in(PlannedCapability::Quests),
        )
        .data::<ModeData>(&[("state_version", Status::Planned)])
        .key(DataTable::MODE, "scripts", Status::Runs(ApiVersion::FIRST))
        .key(DataTable::MODE, "defaults", Status::Runs(ApiVersion::FIRST))
        .data::<TeamData>(&[])
        .data::<RelationData>(&[])
        .data::<PlayersData>(&[])
        .data::<SavesData>(&[])
        .data::<OptionData>(&[])
        .data::<VictoryData>(&[])
        .data::<ModeBountyData>(&[])
        .data::<BountyData>(&[]);
    }

    /// What every role reads of the mode: the teams, the map, the avatars, the units of a tag,
    /// and, for the mode's calls, the players and its state.
    fn register_reads(api: &mut ApiBuilder<'_>) {
        api.call(
            MemberDecl::value("teams", "the playing teams' names, the teams with slots")
                .roles(RoleSet::SIM.and(RoleSet::BOT)),
            |ctx: &mut Ctx| -> Checked<IdNames<Team>> {
                let book = ctx.mode_spec_or_fail()?;
                let teams = &book.teams;
                let names = ctx.view().names();
                let name = |&team| ScriptText::from(names.name(team));
                Ok(teams.playing().iter().map(name).collect())
            },
        )
        .call(
            MemberDecl::value(
                "players",
                "the session's players, by slot, whatever controls each",
            )
            .roles(RoleSet::SIM.and(RoleSet::BOT)),
            |ctx: &mut Ctx| -> Checked<ScriptList> {
                let players = ctx.mode_spec_or_fail()?.teams.players();
                let player = |slot| ScriptValue::of(PlayerHandle::new(PlayerSlot::new(slot)));
                Ok((0..players).map(player).collect())
            },
        )
        .call(
            MemberDecl::value(
                "result",
                "how the match ended, before the call or by its `ctx.win` or `ctx.draw`; `()` while it runs",
            )
            .roles(RoleSet::MODE),
            |ctx: &mut Ctx| -> Checked<ScriptValue> {
                let result = ModeCall::of(&ctx.frame()).result;
                let handle = |result| ScriptValue::of(ResultHandle::new(result, ctx.view().clone()));
                Ok(result.map_or(ScriptValue::UNIT, handle))
            },
        )
        .call(
            MemberDecl::value("map", "the map: its paths and its markers")
                .roles(RoleSet::SIM.and(RoleSet::BOT)),
            |ctx: &mut Ctx| ctx.mode_map_or_fail(),
        )
        .call(
            MemberDecl::value(
                "state",
                "the mode's state fields, by name, to read and write",
            )
            .roles(RoleSet::MODE)
            .own_script(),
            |ctx: &mut Ctx| Ok(StateAccess(ctx.clone())),
        )
        .call(
            MemberDecl::call(
                "enemies",
                &[&["team"]],
                "the playing teams' names that `team` may attack, hostile or neutral, in order",
            )
            .roles(RoleSet::SIM.and(RoleSet::BOT)),
            |ctx: &mut Ctx, team: Team| -> Checked<IdNames<Team>> {
                let book = ctx.mode_spec_or_fail()?;
                let view = ctx.view();
                let enemy = |&&other: &&Team| view.relation(team, other).may_attack();
                let name = |&team| ScriptText::from(view.names().name(team));
                Ok(book.teams.playing().iter().filter(enemy).map(name).collect())
            },
        )
        .call(
            MemberDecl::call(
                "relation",
                &[&["a", "b"]],
                "how team `a` regards team `b`, as the match began the call",
            ),
            |ctx: &mut Ctx, a: Team, b: Team| Ok(ctx.view().relation(a, b)),
        );
        let lists = [
            (
                Listing::Living,
                (
                    "avatars",
                    "the living avatars of every team or of `team`, by stable id",
                ),
                ("units_tagged", "the living units of a tag, by stable id"),
            ),
            (
                Listing::All,
                (
                    "all_avatars",
                    "the avatars, living or dead, of every team or of `team`, by stable id",
                ),
                (
                    "all_units_tagged",
                    "the units of a tag, living or dead, by stable id",
                ),
            ),
        ];
        for (listing, (avatars, of_avatars), (tagged, of_tagged)) in lists {
            let avatars = MemberDecl::call(avatars, &[&[], &["team"]], of_avatars);
            api.call(avatars, move |ctx: &mut Ctx| {
                Ok(Units::avatars(ctx.view(), None, listing))
            })
            .call(avatars, move |ctx: &mut Ctx, team: Team| {
                Ok(Units::avatars(ctx.view(), Some(team), listing))
            })
            .call(
                MemberDecl::call(tagged, &[&["tag"]], of_tagged),
                move |ctx: &mut Ctx, tag: Tag| Ok(ctx.view().units_tagged(tag, listing)),
            );
        }
        api.ty::<StateAccess>()
            .index(|_, state: &mut StateAccess, name: &str| Self::state(&state.0, name))
            .index_set(
                |_, state: &mut StateAccess, name: &str, value: ScriptValue| {
                    Self::set_state(&state.0, name, &value)
                },
            );
    }

    /// The players' choices, which only the mode's calls make and read.
    fn register_choices(api: &mut ApiBuilder<'_>) {
        api.data::<ChoiceData>(&[]).data::<SlotKindData>(&[]);
        let choose = MemberDecl::mode_call(
            "choose",
            &[&["player", "choice", "values"]],
            "records `values`, as many as `choice` takes, each a value it offers, none twice and, \
             in a unique choice, none another player chose, as what `player` chose of it; one \
             value may be given alone",
        )
        .own_script();
        api.call(
            choose,
            |ctx: &mut Ctx, player: PlayerSlot, choice: ChoiceId, values: ScriptList| {
                Self::choose(ctx, player, choice, &values)
            },
        )
        .call(
            choose,
            |ctx: &mut Ctx, player: PlayerSlot, choice: ChoiceId, value: ScriptText| {
                Self::choose(ctx, player, choice, &[ScriptValue::of(value)])
            },
        )
        .call(
            MemberDecl::call(
                "chosen",
                &[&["player", "choice"]],
                "the values `player` chose of `choice`, in order; empty before the player chose",
            )
            .roles(RoleSet::MODE.and(RoleSet::BOT)),
            |ctx: &mut Ctx, player: PlayerSlot, choice: ChoiceId| Self::chosen(ctx, player, choice),
        )
        .call(
            MemberDecl::mode_call(
                "can_choose",
                &[&["player", "choice", "value"]],
                "whether `player` may choose `value` of `choice`: no other player chose it in a \
                 unique choice",
            ),
            |ctx: &mut Ctx, player: PlayerSlot, choice: ChoiceId, value: ScriptText| {
                Self::available(ctx, player, choice, &value)
            },
        )
        .call(
            MemberDecl::call(
                "offers",
                &[&["choice"]],
                "the values `choice` offers, in order: the avatars in the order of the mode's \
                 dependencies, the loadout entries by id, or the values it lists",
            )
            .roles(RoleSet::MODE.and(RoleSet::BOT)),
            |ctx: &mut Ctx, choice: ChoiceId| Self::offers(ctx, choice),
        );
    }

    /// The spawns only the mode's calls make, and the actions they grant.
    fn register_spawns(api: &mut ApiBuilder<'_>) {
        let spawn_unit = MemberDecl::mode_call(
            "spawn_unit",
            &[
                &["type", "team", "position"],
                &["type", "team", "position", "player"],
            ],
            "spawns a unit of `type` on `team` at `position`, within the map's bounds, owned by \
             `player` if given, who plays on `team`, when the call ends, refused where its body \
             would overlap a wall, a body that stands or one the call spawns; the new unit, for \
             `grant` and its `.state`",
        );
        let room_near = MemberDecl::mode_call(
            "room_near",
            &[&["type", "position", "gap", "steps"]],
            "the nearest point to `position`, of the grid of `gap` round it, within `steps` gaps, \
             at most 128, where a unit of `type` spawns `gap` clear of each body and wall; by \
             distance, then by angle from +x as a facing turns; `()` with none",
        );
        let grant = MemberDecl::mode_call(
            "grant",
            &[&["unit", "kind", "ids"]],
            "puts the actions `ids`, loadout entries the mode depends on, in the slot kind \
             `kind` of `unit`, after its slots of that kind, at the kind's first rank",
        )
        .capability(Capability::Abilities);
        api.call(
            spawn_unit,
            |ctx: &mut Ctx, unit_type: UnitType, team: Team, position: ScriptPosition| {
                Self::spawn_unit(ctx, unit_type, team, position.get(), None)
            },
        )
        .call(
            spawn_unit,
            |ctx: &mut Ctx,
             unit_type: UnitType,
             team: Team,
             position: ScriptPosition,
             player: PlayerSlot| {
                Self::spawn_unit(ctx, unit_type, team, position.get(), Some(player))
            },
        )
        .call(
            room_near,
            |ctx: &mut Ctx, unit_type: UnitType, position: ScriptPosition, gap: Num, steps: i64| {
                Self::room_near(ctx, unit_type, position.get(), gap, steps)
            },
        )
        .call(
            grant,
            |ctx: &mut Ctx, unit: Unit, kind: SlotKind, ids: ScriptList| {
                let slots = match unit.at() {
                    UnitAt::Row(row) => ActionsColumn::slot_count(ctx.view(), row),
                    UnitAt::Created(unit_type) => ctx.mode_spec_or_fail()?.spawn_slots(unit_type),
                };
                Self::grant(ctx, unit.id, slots, kind, &ids)
            },
        )
        .call(
            MemberDecl::mode_call(
                "spawn_group",
                &[&["team", "path", "from", "types"]],
                "spawns `types` of `team` in order at the end `from` of `path`, walking it from \
                 there",
            ),
            |ctx: &mut Ctx, team: Team, path: PathId, from: PathEnd, types: ScriptList| {
                Self::spawn_group(ctx, team, path, from, &types)
            },
        );
    }

    /// What only the mode's calls do, but adding resources and setting relations, which every
    /// role does: timers and the match's end.
    fn register_changes(api: &mut ApiBuilder<'_>) {
        let after = MemberDecl::mode_call(
            "after",
            &[&["name", "ms"], &["name", "ms", "data"]],
            "calls `on_timer` with `name`, one the mode declares, and `data`, `()` if not given, \
             `ms` from the call, rounded up to whole ticks, at least one",
        );
        let every = MemberDecl::mode_call(
            "every",
            &[&["name", "ms"], &["name", "ms", "data"]],
            "calls `on_timer` as `after` does, and again each `ms` after, until its cancel",
        );
        api.call(after, |ctx: &mut Ctx, name: TimerId, ms: ScriptTicks| {
            Self::timer(ctx, name, ms, Repeat::Once, &ScriptValue::UNIT)
        })
        .call(
            after,
            |ctx: &mut Ctx, name: TimerId, ms: ScriptTicks, data: ScriptValue| {
                Self::timer(ctx, name, ms, Repeat::Once, &data)
            },
        )
        .call(every, |ctx: &mut Ctx, name: TimerId, ms: ScriptTicks| {
            Self::timer(ctx, name, ms, Repeat::Every, &ScriptValue::UNIT)
        })
        .call(
            every,
            |ctx: &mut Ctx, name: TimerId, ms: ScriptTicks, data: ScriptValue| {
                Self::timer(ctx, name, ms, Repeat::Every, &data)
            },
        )
        .call(
            MemberDecl::mode_call(
                "cancel_timer",
                &[&["name"]],
                "ends every timer of `name` the call's script set, those the call set before among them",
            ),
            |ctx: &mut Ctx, name: TimerId| {
                let script = Self::listed(ctx);
                ctx.queue(ModeEffect::CancelTimer { script, name })
            },
        )
        .call(
            MemberDecl::call(
                "add_resource",
                &[&["player", "name", "amount"]],
                "adds `amount`, at least 0, of the player resource `name`, one the mode declares, to `player`",
            ),
            |ctx: &mut Ctx, player: PlayerSlot, resource: ResourceId, amount: i64| {
                Self::add_resource(ctx, player, resource, amount)
            },
        )
        .call(
            MemberDecl::call(
                "take_resource",
                &[&["player", "name", "amount"]],
                "takes `amount`, at least 0, of the player resource `name` from `player`, which must hold it",
            ),
            |ctx: &mut Ctx, player: PlayerSlot, resource: ResourceId, amount: i64| {
                Self::take_resource(ctx, player, resource, amount)
            },
        )
        .call(
            MemberDecl::mode_call(
                "set_out",
                &[&["player", "out"]],
                "holds `player` out of play while `out`, as when every avatar it holds is dead",
            ),
            |ctx: &mut Ctx, player: PlayerSlot, out: bool| Self::set_out(ctx, player, out),
        )
        .call(
            MemberDecl::mode_call(
                "set_bounty_share",
                &[&["player", "share"]],
                "sets `player`'s share, 0 or more, of the cost of each unit its units kill that a \
                 death pays, as a power raises it, of a mode of a `[bounty]`",
            ),
            |ctx: &mut Ctx, player: PlayerSlot, share: Num| {
                Self::set_bounty_share(ctx, player, share)
            },
        )
        .call(
            MemberDecl::call(
                "set_relation",
                &[&["a", "b", "relation"]],
                "sets how teams `a` and `b` regard each other, their vision as it was",
            ),
            |ctx: &mut Ctx, a: Team, b: Team, relation: Relation| {
                Self::set_relation(ctx, a, b, relation)
            },
        )
        .call(
            MemberDecl::mode_call(
                "win",
                &[&["team"]],
                "ends the match, which `team` wins; a call once it ended does nothing",
            )
            .own_script(),
            |ctx: &mut Ctx, team: Team| Self::end(ctx, MatchResult::Won(team)),
        )
        .call(
            MemberDecl::mode_call(
                "draw",
                &[&[]],
                "ends the match with no winner; a call once it ended does nothing",
            )
            .own_script(),
            |ctx: &mut Ctx| Self::end(ctx, MatchResult::Draw),
        );
    }

    /// `ctx.map`, its paths and its markers.
    fn register_map(api: &mut ApiBuilder<'_>) {
        let field =
            |name, description| MemberDecl::field(ApiHolder::of::<Marker>(), name, description);
        api.ty::<GameMap>()
            .bind(
                MemberDecl::field(ApiHolder::of::<GameMap>(), "paths", "the paths' names")
                    .capability(Capability::Navigation),
                |map: &mut GameMap| map.paths(),
            )
            .method(
                MemberDecl::method(
                    ApiHolder::of::<GameMap>(),
                    "markers",
                    &[&["tag"]],
                    "the markers with `tag`, in the map's order",
                ),
                |map: GameMap, tag: MarkerTag| Ok(map.markers(tag)),
            );
        api.ty::<Marker>()
            .bind(field("name", "its name"), |marker: &mut Marker| {
                marker.info().name.clone()
            })
            .bind(
                field("position", "its point, `()` for a region"),
                |marker: &mut Marker| {
                    marker
                        .info()
                        .position
                        .map_or(ScriptValue::UNIT, ScriptValue::of)
                },
            )
            .bind(
                field("team", "its team's name, `()` with none"),
                |ctx: &Ctx, marker: &mut Marker| {
                    let names = ctx.view().names();
                    let team = marker.info().team;
                    MaybeName::from(team.map(|team| names.name(team)))
                },
            )
            .bind(
                field("params", "its params, by name"),
                |marker: &mut Marker| marker.info().params.clone(),
            );
    }

    /// The place in the mode's list of the script the running call is of, which a mode call
    /// always is.
    fn listed(ctx: &Ctx) -> ListPlace {
        ModeCall::of(&ctx.frame())
            .running()
            .expect("a call of the mode's role runs in a script of its list")
    }

    /// The field `name` of the mode's state, which only its own script holds: a rules script
    /// declares none (03-game-scripting.md, Rules scripts in order).
    fn state_field(ctx: &Ctx, name: &str) -> Checked<StateField> {
        let book = ctx.mode_spec_or_fail()?;
        let field = book.schema.state.field(name);
        let held = ModeCall::of(&ctx.frame()).holds_mode();
        let field = field
            .filter(|_| held)
            .ok_or_else(|| ApiError::UnknownState.fail())?;
        Ok(field)
    }

    fn state(ctx: &Ctx, name: &str) -> Checked<ScriptValue> {
        let field = Self::state_field(ctx, name)?;
        let book = ctx.mode_spec_or_fail()?;
        let frame = ctx.frame();
        let value = &ModeCall::of(&frame).state[field.index()];
        Ok(book.schema.state.read(field, value, ctx.view()))
    }

    fn set_state(ctx: &Ctx, name: &str, value: &ScriptValue) -> Checked<()> {
        let field = Self::state_field(ctx, name)?;
        let value = ctx.mode_spec_or_fail()?.schema.state.take(field, value)?;
        ModeCall::of_mut(&mut *ctx.write()?).state[field.index()] = value;
        Ok(())
    }

    /// Ends the match with `result`, once, in a call of the mode's own script.
    fn end(ctx: &Ctx, result: MatchResult) -> Checked<()> {
        let mut frame = ctx.write()?;
        let call = ModeCall::of_mut(&mut frame);
        if !call.holds_mode() {
            return Err(ModeRefusal::RulesScript.fail().into());
        }
        if call.result.is_some() {
            return Ok(());
        }
        call.result = Some(result);
        frame.queue(ModeEffect::End(result));
        Ok(())
    }

    /// Records `values` as what player `player_slot` chose of `choice`.
    fn choose(
        ctx: &Ctx,
        player_slot: PlayerSlot,
        choice: ChoiceId,
        values: &[ScriptValue],
    ) -> Checked<()> {
        let book = ctx.mode_spec_or_fail()?;
        let choice = book.choices.get(choice);
        if values.len() != choice.count() {
            return Err(ModeRefusal::ChoiceCount.fail().into());
        }
        let mut offers = Vec::with_capacity(values.len());
        for value in values {
            let offer = value
                .clone()
                .into_text()
                .and_then(|id| book.roster.offer_named(choice, id.as_str()))
                .ok_or_else(|| ModeRefusal::UnknownChoiceValue.fail())?;
            if offers.contains(&offer) {
                return Err(ModeRefusal::RepeatedChoiceValue.fail().into());
            }
            offers.push(offer);
        }
        let mut frame = ctx.write()?;
        let call = ModeCall::of_mut(&mut frame);
        if !call.holds_mode() {
            return Err(ModeRefusal::RulesScript.fail().into());
        }
        let taken = |&offer| {
            call.choices
                .is_taken(&book.choices, player_slot, choice, offer)
        };
        if choice.unique && offers.iter().any(taken) {
            return Err(ModeRefusal::ChoiceTaken.fail().into());
        }
        call.choices
            .choose(&book.choices, player_slot, choice, &offers);
        Ok(())
    }

    /// The values player `player_slot` chose of `choice`, in order, empty before the player chose.
    fn chosen(ctx: &Ctx, player_slot: PlayerSlot, choice: ChoiceId) -> Checked<ScriptList> {
        let book = ctx.mode_spec_or_fail()?;
        let choice = book.choices.get(choice);
        let frame = ctx.frame();
        let Some(values) = ModeCall::of(&frame)
            .choices
            .chosen(&book.choices, player_slot, choice)
        else {
            return Ok(ScriptList::default());
        };
        Ok(values
            .map(|offer| ScriptValue::of(ScriptText::from(book.roster.id(choice, offer))))
            .collect())
    }

    /// The values `choice` offers, in order.
    fn offers(ctx: &Ctx, choice: ChoiceId) -> Checked<ScriptList> {
        let book = ctx.mode_spec_or_fail()?;
        let choice = book.choices.get(choice);
        Ok(book
            .roster
            .ids(choice)
            .map(|id| ScriptValue::of(ScriptText::from(id)))
            .collect())
    }

    /// Whether player `player_slot` may choose `value` of `choice`: it offers the value, and no
    /// other player chose it in a unique choice.
    fn available(
        ctx: &Ctx,
        player_slot: PlayerSlot,
        choice: ChoiceId,
        value: &str,
    ) -> Checked<bool> {
        let book = ctx.mode_spec_or_fail()?;
        let choice = book.choices.get(choice);
        let offer = book
            .roster
            .offer_named(choice, value)
            .ok_or_else(|| ModeRefusal::UnknownChoiceValue.fail())?;
        let taken =
            ModeCall::of(&ctx.frame())
                .choices
                .is_taken(&book.choices, player_slot, choice, offer);
        Ok(!(choice.unique && taken))
    }

    /// `unit_type`, when `book` spawns it.
    fn spawnable(book: &ModeSpec, unit_type: UnitType) -> Checked<UnitType> {
        if !book.spawns(unit_type) {
            return Err(ApiError::Unknown(UnitType::NAMES).fail().into());
        }
        Ok(unit_type)
    }

    /// Queues the spawn of a unit of `unit_type` on `team` at `position`, owned by `owner` if it
    /// names one, who plays on `team`: the new unit, with the id the call takes for it.
    fn spawn_unit(
        ctx: &Ctx,
        unit_type: UnitType,
        team: Team,
        position: Vec3,
        owner: Option<PlayerSlot>,
    ) -> Checked<Unit> {
        let book = ctx.mode_spec_or_fail()?;
        if !ctx.view().bounds().contains(position) {
            return Err(ModeRefusal::OutOfBounds.fail().into());
        }
        let unit_type = Self::spawnable(&book, unit_type)?;
        let body = book.body(unit_type).map(|form| form.at(Num::ZERO));
        if let Some(body) = body {
            let ask = RoomAsk {
                position,
                shape: body.shape(),
                layer: body.layer(),
                gap: Num::ZERO,
            };
            let room = {
                let frame = ctx.frame();
                NavigationColumn::room_for(ctx.view(), ask, &ModeCall::of(&frame).bodies)
            };
            if !room {
                return Err(ModeRefusal::NoRoom.fail().into());
            }
        }
        if owner.is_some_and(|slot| book.teams.of(slot) != Some(team)) {
            return Err(ModeRefusal::PlayerOffTeam.fail().into());
        }
        let id = ctx.write()?.take_id();
        if let Some(body) = body {
            let spawning = IndexedBody::of(id, position, &body);
            ModeCall::of_mut(&mut *ctx.write()?).bodies.push(spawning);
        }
        let spawn = SpawnAt {
            id,
            unit_type,
            team,
            position,
            owner,
            angle: Num::ZERO,
            variation: None,
        };
        ctx.queue(ModeEffect::SpawnUnit {
            at: spawn,
            grants: Vec::new(),
        })?;
        Ok(Unit::created(id, unit_type, ctx.view().clone()))
    }

    /// The nearest point to `position` where a unit of `unit_type` has room, `gap` clear of each
    /// body and wall, on the grid of `gap` round it, within `steps` gaps, as `spawn_unit` then
    /// spawns it there; `()` with none. A unit with no body takes the room of a point.
    fn room_near(
        ctx: &Ctx,
        unit_type: UnitType,
        position: Vec3,
        gap: Num,
        steps: i64,
    ) -> Checked<ScriptValue> {
        let book = ctx.mode_spec_or_fail()?;
        let unit_type = Self::spawnable(&book, unit_type)?;
        let steps = u32::try_from(steps)
            .ok()
            .filter(|&steps| steps <= GridSearch::MAX_STEPS && gap > Num::ZERO)
            .ok_or_else(|| ModeRefusal::RoomSearch.fail())?;
        let body = book.body(unit_type).map(|form| form.at(Num::ZERO));
        let search = RoomSearch {
            grid: GridSearch {
                position,
                gap,
                steps,
            },
            shape: Body::shape_of(body.as_ref()),
            layer: Body::layer_of(body.as_ref()),
        };
        let frame = ctx.frame();
        let found = NavigationColumn::room_near(ctx.view(), search, &ModeCall::of(&frame).bodies);
        Ok(found.map_or(ScriptValue::UNIT, ScriptValue::of))
    }

    /// Queues the actions `ids` into the slot kind `kind` of `id`, which has `slots` slots
    /// before the grants this call queued: with the spawn of a unit the call spawns, so it
    /// spawns with them, or on its own for a unit that stands.
    fn grant(
        ctx: &Ctx,
        id: StableId,
        slots: usize,
        kind: SlotKind,
        ids: &[ScriptValue],
    ) -> Checked<()> {
        let book = ctx.mode_spec_or_fail()?;
        let view = ctx.view();
        if ActionsColumn::kind_ranks(view, kind) != book.loadout_ranks {
            return Err(ModeRefusal::SlotKindRanks.fail().into());
        }
        let rank = ActionsColumn::first_rank(view, kind);
        let abilities = ids
            .iter()
            .map(|name| {
                let name = name.clone().into_text();
                name.and_then(|name| book.roster.loadout_ability_named(name.as_str()))
                    .ok_or_else(|| ApiError::Unknown(ActionId::NAMES).fail().into())
            })
            .collect::<Checked<Vec<_>>>()?;
        let mut frame = ctx.write()?;
        let queued: usize = frame
            .queued::<ModeEffect>()
            .map(|effect| match effect {
                ModeEffect::Grant { id: granted, grant } if *granted == id => grant.abilities.len(),
                ModeEffect::SpawnUnit { at, grants } if at.id == id => {
                    grants.iter().map(|grant| grant.abilities.len()).sum()
                }
                _ => 0,
            })
            .sum();
        if slots + queued + abilities.len() > ActionSlots::LIMIT {
            return Err(ModeRefusal::TooManySlots.fail().into());
        }
        let grant = SlotGrant {
            kind,
            rank,
            abilities,
        };
        let spawn = frame
            .queued_mut::<ModeEffect>()
            .find_map(|effect| match effect {
                ModeEffect::SpawnUnit { at, grants } if at.id == id => Some(grants),
                _ => None,
            });
        match spawn {
            Some(grants) => grants.push(grant),
            None => frame.queue(ModeEffect::Grant { id, grant }),
        }
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
        let book = ctx.mode_spec_or_fail()?;
        let types = types
            .iter()
            .map(|name| {
                let name = name.clone().into_text();
                let name = name.ok_or_else(|| ApiError::Unknown(UnitType::NAMES).fail())?;
                Self::spawnable(&book, UnitType::take_text(name, &ArgCtx::Ctx(ctx))?)
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
    /// Sets the timer `name`, due `ms` after the call, and each `ms` after when `repeat` says so.
    fn timer(
        ctx: &Ctx,
        name: TimerId,
        ms: ScriptTicks,
        repeat: Repeat,
        data: &ScriptValue,
    ) -> Checked<()> {
        let ticks = ms.get();
        let data = Self::timer_data(data).map_err(ApiError::fail)?;
        ctx.queue(ModeEffect::Timer(TimerStart {
            script: Self::listed(ctx),
            name,
            after: ticks,
            every: (repeat == Repeat::Every).then_some(ticks),
            data,
        }))
    }

    /// Queues a change of how teams `a` and `b`, two of the mode's, regard each other.
    fn set_relation(ctx: &Ctx, a: Team, b: Team, relation: Relation) -> Checked<()> {
        if a == b {
            return Err(ModeRefusal::SelfRelation.fail().into());
        }
        ctx.queue(ModeEffect::SetRelation { a, b, relation })
    }

    /// Whether a player or a bot held `player_slot` as the match started.
    fn is_held(ctx: &Ctx, player_slot: PlayerSlot) -> bool {
        let frame = ctx.frame();
        let players = frame
            .extension::<PlayersCall>()
            .expect("every call holds the players'");
        players.holder(player_slot) == Some(PlayerSlotHolder::Held)
    }

    /// The living units of player `player_slot` that the view's viewer sees, by stable id.
    fn units_of(ctx: &Ctx, player_slot: PlayerSlot) -> ScriptList {
        ctx.view()
            .units_where(|row| row.alive && row.owner == Some(player_slot))
    }

    /// Queues player `player_slot` held out of play, or no more.
    fn set_out(ctx: &Ctx, player_slot: PlayerSlot, out: bool) -> Checked<()> {
        ctx.queue(ModeEffect::SetOut { player_slot, out })
    }

    /// Sets player `player_slot`'s bounty share; one below 0, or a mode of no `[bounty]`, fails the
    /// call.
    fn set_bounty_share(ctx: &Ctx, player_slot: PlayerSlot, share: Num) -> Checked<()> {
        if share < Num::ZERO {
            return Err(ModeRefusal::NegativeShare.fail().into());
        }
        if !ctx.mode_spec_or_fail()?.bounty {
            return Err(ModeRefusal::NoBounty.fail().into());
        }
        ctx.queue(ModeEffect::SetBountyShare { player_slot, share })
    }

    /// The `Player` handle's reads of what the mode keeps: its team, its units, its player slot,
    /// its play and its resources.
    fn register_player(api: &mut ApiBuilder<'_>) {
        let owner = ApiHolder::of::<PlayerHandle>();
        let field = |name, description| MemberDecl::field(owner, name, description);
        api.bind(
            field("team", "its team's name"),
            |ctx: &Ctx, player: &mut PlayerHandle| -> Checked<IdName<Team>> {
                let book = ctx.mode_spec_or_fail()?;
                let team = book.teams.of(player.player_slot());
                let team = team.expect("a player of the session has a team");
                Ok(ctx.view().names().name(team))
            },
        )
        .bind(
            field(
                "state",
                "its script state, by name, as the mode's `[player_state]` declares it, which a call may write and read back",
            ),
            |player: &mut PlayerHandle| PlayerStateAccess(player.player_slot()),
        )
        .bind(
            field("units", "the living units it owns, by stable id"),
            |ctx: &Ctx, player: &mut PlayerHandle| Self::units_of(ctx, player.player_slot()),
        )
        .bind(
            field(
                "held",
                "whether a player or a bot held its slot as the match started",
            ),
            |ctx: &Ctx, player: &mut PlayerHandle| Self::is_held(ctx, player.player_slot()),
        )
        .bind(
            field(
                "is_out",
                "whether it is out of play, as the last tick's end derived it",
            ),
            |ctx: &Ctx, player: &mut PlayerHandle| {
                let frame = ctx.frame();
                let players = frame.extension::<PlayersCall>();
                let out = players.and_then(|players| players.is_out(player.player_slot()));
                out.expect("a call holds each player of the session")
            },
        )
        .method(
            MemberDecl::method(
                owner,
                "resource",
                &[&["name"]],
                "the amount of the player resource `name` it holds, with the call's changes",
            ),
            |ctx: &Ctx, player: PlayerSlot, resource: ResourceId| {
                ctx.view().admit_player(player, DataKind::Resources)?;
                let frame = ctx.frame();
                let players = frame
                    .extension::<PlayersCall>()
                    .expect("every call holds the players'");
                let resources = players
                    .resources(player)
                    .expect("a mode that names a resource keeps each player's resources");
                Ok(resources.amount(resource))
            },
        );
    }

    /// Adds `amount` of `resource` to player `player_slot`'s; a negative amount, or a sum past an
    /// integer, fails the call.
    fn add_resource(
        ctx: &Ctx,
        player_slot: PlayerSlot,
        resource: ResourceId,
        amount: i64,
    ) -> Checked<()> {
        if amount < 0 {
            return Err(ModeRefusal::NegativeAmount.fail().into());
        }
        let mut frame = ctx.write()?;
        Self::resources_mut(&mut frame, player_slot)
            .add(resource, amount)
            .ok_or_else(|| ModeRefusal::ResourceOverflow.fail().into())
    }

    /// Takes `amount` of `resource` from player `player_slot`'s; a negative amount, or more than
    /// the player holds, fails the call.
    fn take_resource(
        ctx: &Ctx,
        player_slot: PlayerSlot,
        resource: ResourceId,
        amount: i64,
    ) -> Checked<()> {
        if amount < 0 {
            return Err(ModeRefusal::NegativeAmount.fail().into());
        }
        let mut frame = ctx.write()?;
        let resources = Self::resources_mut(&mut frame, player_slot);
        if resources.amount(resource) < amount {
            return Err(ModeRefusal::ResourceShort.fail().into());
        }
        resources
            .add(resource, -amount)
            .expect("a take of what the player holds");
        Ok(())
    }

    /// Player `player_slot`'s resources, for the call to change.
    fn resources_mut(frame: &mut Frame, player_slot: PlayerSlot) -> &mut PlayerResources {
        frame
            .extension_mut::<PlayersCall>()
            .expect("every call holds the players'")
            .resources_mut(player_slot)
            .expect("a mode that names a resource keeps each player's resources")
    }

    /// Timer data as state holds it: `None` for `()`, or the value of the first type that takes
    /// it.
    fn timer_data(data: &ScriptValue) -> Result<Option<StateValue>, ApiError> {
        if data.is_unit() {
            return Ok(None);
        }
        StateType::timer_order()
            .find_map(|kind| StateValue::of_kind(kind, data))
            .map(Some)
            .ok_or_else(|| ModeRefusal::TimerData.refused())
    }
}

impl ScriptHandle for StateAccess {
    const NAME: &'static str = "ModeState";
    const TITLE: &'static str = "The mode's state";
}
