use std::rc::Rc;
use std::sync::Arc;

use bevy_ecs::world::World;
use campfire_navigation::IndexedBody;
use campfire_script::{
    CallError, CallParams, CallPart, CallStart, Commit, Frame, ListPlace, ParamScope, ScriptRole,
    ScriptValue, StateValue,
};
use campfire_units::{SyncAll, SyncNone};
use campfire_values::Scalar;

use crate::choices::Choices;
use crate::game_map::GameMap;
use crate::match_end::{MatchEnd, MatchResult};
use crate::mode_script::ModeScript;
use crate::mode_spec::ModeSpec;
use crate::mode_world::{ModeWorld, ModeWorldCrate};
use crate::player_states::PlayerStates;

/// What the mode adds to the call frame: the mode's params, which a mode's or an AI's call reads;
/// the players' state, which every call reads and writes back; as a call of the mode's scripts
/// sees them, the players' choices and how the match ended, before the call or in it; as a call of
/// the mode's own script sees it, the mode's state, which it writes and reads back, and which
/// commits with the choices it makes before its effects apply; and the boxes the batch's calls
/// spawned, which a box spawned next needs room from, as the batch's view holds none of them.
#[derive(Debug)]
pub(crate) struct ModeCall {
    book: Arc<ModeSpec>,
    /// The mode's values as scripts read them.
    script: Rc<ModeScript>,
    /// The place in the mode's list of the script the running call is of, none for another role.
    running: Option<ListPlace>,
    /// Every field of the mode's state, in the order of their names.
    pub(crate) state: Vec<StateValue>,
    /// The fields of each class, `none` and `all`, as a commit splits them.
    none: Vec<StateValue>,
    all: Vec<StateValue>,
    pub(crate) choices: Choices,
    pub(crate) result: Option<MatchResult>,
    /// The boxes the batch's committed calls spawned, then the running call's.
    pub(crate) boxes: Vec<IndexedBody>,
    pub(crate) players: PlayerStates,
    /// How many of `boxes` the batch's committed calls spawned.
    committed: usize,
}

impl CallPart for ModeCall {
    /// Before the effects, as an effect may read the mode's state and the choices.
    const COMMITS: Commit = Commit::BeforeEffects;

    /// A batch holds no box an earlier batch's calls spawned, which the batch's view holds; an
    /// earlier call of the batch changes the mode's state and the choices.
    fn begin_batch(&mut self, world: &World) {
        self.boxes.clear();
        self.committed = 0;
        let players = self.book.teams.players();
        self.players.read(world, &self.book.schema.players, players);
    }

    /// A call of the mode's scripts reads the choices and the match's end as `world` holds them,
    /// and one of its own script the mode's state too.
    fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError> {
        self.players.begin();
        self.running = start.listed.filter(|_| start.role == ScriptRole::Mode);
        if self.running.is_none() {
            return Ok(());
        }
        if self.holds_mode() {
            let (none, all) = (
                world.mode_fields::<SyncNone>(),
                world.mode_fields::<SyncAll>(),
            );
            self.book.schema.merge_state(none, all, &mut self.state);
        }
        self.choices.clone_from(world.resource::<Choices>());
        self.result = world.match_end().map(MatchEnd::result);
        self.boxes.truncate(self.committed);
        Ok(())
    }

    /// Writes the state of a call of the mode's own script, each class of it unless it is what
    /// `world` holds, and its choices unless they are, as a write marks them changed.
    fn commit(&mut self, world: &mut World) {
        self.players.commit(world, &self.book.schema.players);
        if self.running.is_none() {
            return;
        }
        self.committed = self.boxes.len();
        if !self.holds_mode() {
            return;
        }
        let (none, all) = (&mut self.none, &mut self.all);
        self.book.schema.split_state(&self.state, none, all);
        world.commit_mode_fields::<SyncNone>(none);
        world.commit_mode_fields::<SyncAll>(all);
        if *world.resource::<Choices>() != self.choices {
            world.resource_mut::<Choices>().clone_from(&self.choices);
        }
    }
}

impl CallParams for ModeCall {
    const SCOPE: ParamScope = ParamScope::Mode;

    fn param(&self, name: &str) -> Option<ScriptValue> {
        self.script.param_named(name)
    }

    /// None: a mode's params are read by name alone, as no effect list of the mode's numbers
    /// a param by its place.
    fn param_at(&self, _: usize) -> Option<Scalar> {
        None
    }
}

impl ModeCall {
    pub(crate) fn new(book: Arc<ModeSpec>, script: Rc<ModeScript>) -> Self {
        Self {
            book,
            script,
            running: None,
            state: Vec::new(),
            none: Vec::new(),
            all: Vec::new(),
            choices: Choices::default(),
            result: None,
            boxes: Vec::new(),
            committed: 0,
            players: PlayerStates::default(),
        }
    }

    /// The place in the mode's list of the script the running call is of, when it is a mode's.
    pub(crate) const fn running(&self) -> Option<ListPlace> {
        self.running
    }

    /// Whether the running call is of the mode's own script, which holds the mode's state and
    /// choices, and ends the match (03-game-scripting.md, Rules scripts in order).
    pub(crate) fn holds_mode(&self) -> bool {
        self.running == Some(self.book.schema.mode)
    }

    /// The match's mode.
    pub(crate) fn spec(&self) -> Arc<ModeSpec> {
        Arc::clone(&self.book)
    }

    /// `ctx.map`.
    pub(crate) fn map(&self) -> GameMap {
        self.script.map()
    }

    /// The mode's part of `frame`, which the mode added as it installed.
    pub(crate) fn of(frame: &Frame) -> &Self {
        frame
            .part()
            .expect("a match whose mode installed has its part of the frame")
    }

    /// The mode's part of `frame`, to change.
    pub(crate) fn of_mut(frame: &mut Frame) -> &mut Self {
        frame
            .part_mut()
            .expect("a match whose mode installed has its part of the frame")
    }
}
