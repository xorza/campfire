use std::ops::Range;

use campfire_math::Num;
use campfire_sim::StableId;

use crate::abilities::ability_book::AbilityId;
use crate::units::error::CallError;
use crate::units::scalar::Scalar;

/// What `on_cast` calls read and queue, beside the units the view holds. One frame serves the
/// whole match, its buffers cleared and filled again, so a cast allocates none of them.
#[derive(Debug)]
pub(crate) struct Frame {
    /// The param names of every loaded ability, sorted, one run per ability.
    param_names: Vec<Box<str>>,
    /// Where the run of each ability's names starts, by ability id, then where the last one ends.
    param_starts: Vec<u32>,
    /// The running cast's run of names.
    cast_names: Range<usize>,
    /// The running cast's params at its rank, in the order of its names.
    params: Vec<Scalar>,
    /// The effects the running cast queued, in order.
    pub(crate) effects: Vec<Effect>,
}

/// An effect a call queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Effect {
    Damage { target: StableId, amount: Num },
}

impl Default for Frame {
    fn default() -> Frame {
        Frame {
            param_names: Vec::new(),
            param_starts: vec![0],
            cast_names: 0..0,
            params: Vec::new(),
            effects: Vec::new(),
        }
    }
}

impl Frame {
    /// Adds the param names of the ability the book loads next, sorted, as the book keeps its
    /// params.
    pub(crate) fn add_param_names<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) {
        let start = self.param_names.len();
        self.param_names.extend(names.into_iter().map(Box::from));
        debug_assert!(self.param_names[start..].is_sorted());
        let end = u32::try_from(self.param_names.len()).expect("param names fit u32");
        self.param_starts.push(end);
    }

    /// Starts a cast of `ability`, with its params at the cast's rank in the order of its names;
    /// a param that overflows at that rank fails the cast.
    pub(crate) fn begin_cast(
        &mut self,
        ability: AbilityId,
        params: impl IntoIterator<Item = Option<Scalar>>,
    ) -> Result<(), CallError> {
        let index = ability.index();
        self.cast_names = self.param_starts[index] as usize..self.param_starts[index + 1] as usize;
        self.effects.clear();
        self.params.clear();
        self.params.reserve_exact(self.cast_names.len());
        for param in params {
            self.params.push(param.ok_or(CallError::ParamOverflow)?);
        }
        debug_assert_eq!(self.params.len(), self.cast_names.len());
        Ok(())
    }

    /// The running cast's param `name`, if its ability declares one.
    pub(crate) fn param(&self, name: &str) -> Option<Scalar> {
        let names = &self.param_names[self.cast_names.clone()];
        let index = names.binary_search_by(|probe| (**probe).cmp(name)).ok()?;
        Some(self.params[index])
    }
}
