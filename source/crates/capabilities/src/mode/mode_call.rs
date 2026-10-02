use std::rc::Rc;

use bevy_ecs::world::World;
use campfire_script::rhai::Dynamic;

use crate::mode::choices::Choices;
use crate::mode::match_end::MatchEnd;
use crate::mode::mode_book::ModeBook;
use crate::mode::mode_state::ModeState;
use crate::scripts::call_part::CallPart;
use crate::scripts::call_start::CallStart;
use crate::scripts::error::CallError;
use crate::scripts::frame::Frame;
use crate::scripts::script_role::ScriptRole;
use crate::scripts::state_value::StateValue;

/// What the mode adds to the call frame: the mode's params, which a mode's or an AI's call reads;
/// and as a mode call sees them, the mode's state and the players' choices, which it writes and
/// reads back, and whether the match ended, before the call or in it.
#[derive(Debug)]
pub(crate) struct ModeCall {
    book: Rc<ModeBook>,
    pub(crate) state: Vec<StateValue>,
    pub(crate) choices: Choices,
    pub(crate) ended: bool,
}

impl CallPart for ModeCall {
    /// A mode call reads the mode's state, the choices and the match's end as `world` holds
    /// them.
    fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError> {
        if start.role != ScriptRole::Mode {
            return Ok(());
        }
        self.state.clear();
        self.state
            .extend_from_slice(&world.resource::<ModeState>().0);
        self.choices.clone_from(world.resource::<Choices>());
        self.ended = world.contains_resource::<MatchEnd>();
        Ok(())
    }

    /// A mode's or an AI's call reads the mode's params.
    fn param_named(&self, role: Option<ScriptRole>, name: &str) -> Option<Dynamic> {
        if matches!(role, Some(ScriptRole::Action | ScriptRole::Modifier)) {
            return None;
        }
        self.book.schema.param_named(name)
    }

    /// A mode call commits its state and choices itself, before its effects apply.
    fn apply(&mut self, _: &mut World) {}
}

impl ModeCall {
    pub(crate) fn new(book: Rc<ModeBook>) -> ModeCall {
        ModeCall {
            book,
            state: Vec::new(),
            choices: Choices::default(),
            ended: false,
        }
    }

    /// The mode's part of `frame`, which the mode added as it installed.
    pub(crate) fn of(frame: &Frame) -> &ModeCall {
        frame
            .part()
            .expect("a match whose mode installed has its part of the frame")
    }

    /// The mode's part of `frame`, to change.
    pub(crate) fn of_mut(frame: &mut Frame) -> &mut ModeCall {
        frame
            .part_mut()
            .expect("a match whose mode installed has its part of the frame")
    }
}
