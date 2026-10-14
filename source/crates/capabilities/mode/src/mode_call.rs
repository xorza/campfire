use std::rc::Rc;

use bevy_ecs::world::World;
use campfire_navigation::IndexedBody;
use campfire_script::{CallError, CallPart, CallStart, Frame, ScriptRole, ScriptValue, StateValue};
use campfire_units::{SyncAll, SyncNone};

use crate::choices::Choices;
use crate::mode_spec::ModeSpec;
use crate::mode_world::{ModeWorld, ModeWorldCrate};

/// What the mode adds to the call frame: the mode's params, which a mode's or an AI's call reads;
/// and as a mode call sees them, the mode's state and the players' choices, which it writes and
/// reads back, whether the match ended, before the call or in it, and the boxes it spawned, which
/// a box it spawns next needs room from.
#[derive(Debug)]
pub struct ModeCall {
    book: Rc<ModeSpec>,
    /// Every field of the mode's state, in the order of their names.
    pub(crate) state: Vec<StateValue>,
    /// The fields of each class, `none` and `all`, as a commit splits them.
    none: Vec<StateValue>,
    all: Vec<StateValue>,
    pub(crate) choices: Choices,
    pub(crate) ended: bool,
    pub(crate) boxes: Vec<IndexedBody>,
}

impl CallPart for ModeCall {
    /// A mode call reads the mode's state, the choices and the match's end as `world` holds
    /// them.
    fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError> {
        if start.role != ScriptRole::Mode {
            return Ok(());
        }
        let (none, all) = (
            world.mode_fields::<SyncNone>(),
            world.mode_fields::<SyncAll>(),
        );
        self.book.schema.merge_state(none, all, &mut self.state);
        self.choices.clone_from(world.resource::<Choices>());
        self.ended = world.match_end().is_some();
        self.boxes.clear();
        Ok(())
    }

    /// A mode's or an AI's call reads the mode's params.
    fn param(&self, role: Option<ScriptRole>, name: &str) -> Option<ScriptValue> {
        if matches!(role, Some(ScriptRole::Action | ScriptRole::Modifier)) {
            return None;
        }
        self.book.schema.param(name)
    }

    /// A mode call commits its state and choices itself, before its effects apply.
    fn apply(&mut self, _: &mut World) {}
}

impl ModeCall {
    pub fn new(book: Rc<ModeSpec>) -> ModeCall {
        ModeCall {
            book,
            state: Vec::new(),
            none: Vec::new(),
            all: Vec::new(),
            choices: Choices::default(),
            ended: false,
            boxes: Vec::new(),
        }
    }

    /// Writes the call's state, each class of it unless it is what `world` holds.
    pub(crate) fn commit_state(&mut self, world: &mut World) {
        let (none, all) = (&mut self.none, &mut self.all);
        self.book.schema.split_state(&self.state, none, all);
        world.commit_mode_fields::<SyncNone>(none);
        world.commit_mode_fields::<SyncAll>(all);
    }

    /// The mode's part of `frame`, which the mode added as it installed.
    pub fn of(frame: &Frame) -> &ModeCall {
        frame
            .part()
            .expect("a match whose mode installed has its part of the frame")
    }

    /// The mode's part of `frame`, to change.
    pub fn of_mut(frame: &mut Frame) -> &mut ModeCall {
        frame
            .part_mut()
            .expect("a match whose mode installed has its part of the frame")
    }
}
