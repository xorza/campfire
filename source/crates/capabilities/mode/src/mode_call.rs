use std::rc::Rc;

use bevy_ecs::world::World;
use campfire_navigation::IndexedBody;
use campfire_script::{
    CallError, CallParams, CallPart, CallStart, Commit, Frame, ParamScope, ScriptRole, ScriptValue,
    StateValue,
};
use campfire_units::{SyncAll, SyncNone};

use crate::choices::Choices;
use crate::mode_spec::ModeSpec;
use crate::mode_world::{ModeWorld, ModeWorldCrate};

/// What the mode adds to the call frame: the mode's params, which a mode's or an AI's call reads;
/// and as a mode call sees them, the mode's state and the players' choices, which it writes and
/// reads back and which commit before its effects apply, whether the match ended, before the call
/// or in it, and the boxes the batch's calls spawned, which a box spawned next needs room from, as
/// the batch's view holds none of them.
#[derive(Debug)]
pub struct ModeCall {
    book: Rc<ModeSpec>,
    /// Whether the running call is the mode's, whose state and choices it holds.
    running: bool,
    /// Every field of the mode's state, in the order of their names.
    pub(crate) state: Vec<StateValue>,
    /// The fields of each class, `none` and `all`, as a commit splits them.
    none: Vec<StateValue>,
    all: Vec<StateValue>,
    pub(crate) choices: Choices,
    pub(crate) ended: bool,
    /// The boxes the batch's committed calls spawned, then the running call's.
    pub(crate) boxes: Vec<IndexedBody>,
    /// How many of `boxes` the batch's committed calls spawned.
    committed: usize,
}

impl CallPart for ModeCall {
    /// Before the effects, as an effect may read the mode's state and the choices.
    const COMMITS: Commit = Commit::BeforeEffects;

    /// A mode call reads the mode's state, the choices and the match's end as `world` holds
    /// them.
    fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError> {
        self.running = start.role == ScriptRole::Mode;
        if !self.running {
            return Ok(());
        }
        let (none, all) = (
            world.mode_fields::<SyncNone>(),
            world.mode_fields::<SyncAll>(),
        );
        self.book.schema.merge_state(none, all, &mut self.state);
        self.choices.clone_from(world.resource::<Choices>());
        self.ended = world.match_end().is_some();
        self.boxes.truncate(self.committed);
        Ok(())
    }

    /// Writes a mode call's state, each class of it unless it is what `world` holds, and its
    /// choices unless they are, as a write marks them changed.
    fn commit(&mut self, world: &mut World) {
        if !self.running {
            return;
        }
        let (none, all) = (&mut self.none, &mut self.all);
        self.book.schema.split_state(&self.state, none, all);
        world.commit_mode_fields::<SyncNone>(none);
        world.commit_mode_fields::<SyncAll>(all);
        if *world.resource::<Choices>() != self.choices {
            world.resource_mut::<Choices>().clone_from(&self.choices);
        }
        self.committed = self.boxes.len();
    }
}

impl CallParams for ModeCall {
    const SCOPE: ParamScope = ParamScope::Mode;

    fn param(&self, name: &str) -> Option<ScriptValue> {
        self.book.schema.param_named(name)
    }
}

impl ModeCall {
    pub fn new(book: Rc<ModeSpec>) -> Self {
        Self {
            book,
            running: false,
            state: Vec::new(),
            none: Vec::new(),
            all: Vec::new(),
            choices: Choices::default(),
            ended: false,
            boxes: Vec::new(),
            committed: 0,
        }
    }

    /// Ends a batch of calls: the next batch's view holds every box its calls spawned.
    pub(crate) fn end_batch(&mut self) {
        self.boxes.clear();
        self.committed = 0;
    }

    /// Asserts that no batch holds boxes, as one starts.
    pub(crate) fn debug_assert_no_batch(&self) {
        debug_assert!(
            self.boxes.is_empty() && self.committed == 0,
            "the last batch ended"
        );
    }

    /// The mode's part of `frame`, which the mode added as it installed.
    pub fn of(frame: &Frame) -> &Self {
        frame
            .part()
            .expect("a match whose mode installed has its part of the frame")
    }

    /// The mode's part of `frame`, to change.
    pub fn of_mut(frame: &mut Frame) -> &mut Self {
        frame
            .part_mut()
            .expect("a match whose mode installed has its part of the frame")
    }
}
