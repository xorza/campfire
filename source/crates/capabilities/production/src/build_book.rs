use std::ops::Range;

use bevy_ecs::resource::Resource;
use campfire_actions::{ActionBook, ActionSlots, LoadedAction};
use campfire_declarations::{Book, BookCx, BookError, ById};
use campfire_math::Num;
use campfire_script::{Filter, MatchNames, UnknownName};
use campfire_units::{BodyForm, Owner, UnitTypes};
use campfire_values::{ActionId, Named, Share, TypeScope, UnitType};

use crate::book_builders;
use crate::build_data::BuildData;
use crate::build_start::BuildStart;
use crate::construct_data::ConstructData;
use crate::error::ProductionDataError;
use crate::placement_data::{PlacementData, PlacementRule};

/// What each build places, when it starts and how its site grows, by action, its rates and its
/// placement's rules one run after another: package data, not state.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
#[component(immutable)]
pub struct BuildBook {
    /// By action, as the book loads them in order.
    specs: Vec<Stored>,
    rates: Vec<Num>,
    rules: Vec<PlacementCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Stored {
    action: ActionId,
    unit_type: UnitType,
    form: BodyForm,
    start: BuildStart,
    style: StoredStyle,
    start_life: Option<Share>,
    refund: Share,
    near: Range<u32>,
    away: Range<u32>,
}

/// How a site grows: by itself, while one builder builds it, or at its builders' rate, from the
/// rate for one builder, one rate at least.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style<'a> {
    Alone,
    Builder,
    Builders(&'a [Num]),
}

/// A `Style` as `BuildBook` holds it: its builders' rates a run of its rates.
#[derive(Debug, Clone, PartialEq, Eq)]
enum StoredStyle {
    Alone,
    Builder,
    Builders(Range<u32>),
}

/// An entry of a placement: a living unit the filter selects for the builder, within `distance`
/// of the box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacementCheck {
    pub filter: Filter,
    pub distance: Num,
}

/// One build's spec, as `BuildBook` holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildSpec<'a> {
    /// The building's type, a type of its action's package that stands.
    pub unit_type: UnitType,
    /// The building's box, before its angle turns it.
    pub(crate) form: BodyForm,
    pub(crate) start: BuildStart,
    pub(crate) style: Style<'a>,
    /// The share of its life pool's maximum a site starts with, none for a type without it.
    pub(crate) start_life: Option<Share>,
    /// The share of what the build paid that a cancel returns.
    pub(crate) refund: Share,
    pub(crate) near: &'a [PlacementCheck],
    pub(crate) away: &'a [PlacementCheck],
}

impl BuildSpec<'_> {
    /// Whether a builder of `owner` continues a site of `site_type` and `site_owner` with it: a
    /// site of its building and its player, as a build order at a site takes one.
    pub(crate) fn continues(
        &self,
        site_type: UnitType,
        site_owner: Option<&Owner>,
        owner: Option<&Owner>,
    ) -> bool {
        site_type == self.unit_type && site_owner == owner
    }
}

/// A builder's build of a slot: its spec, the building's type among it, and its range.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Building<'a> {
    pub(crate) spec: BuildSpec<'a>,
    pub(crate) range: Num,
}

/// What one build loads with.
#[derive(Debug, Clone)]
pub struct NewBuild<'a> {
    pub unit_type: UnitType,
    pub form: BodyForm,
    pub start: BuildStart,
    pub style: Style<'a>,
    pub start_life: Option<Share>,
    pub refund: Share,
    pub near: Vec<PlacementCheck>,
    pub away: Vec<PlacementCheck>,
}

impl BuildBook {
    /// Gives `action`, loaded after every action it holds, its build.
    pub fn push(&mut self, action: ActionId, build: NewBuild<'_>) {
        debug_assert!(self.specs.last().is_none_or(|last| last.action < action));
        let run = |at: usize| u32::try_from(at).expect("a book's builds fit a u32");
        let style = match build.style {
            Style::Alone => StoredStyle::Alone,
            Style::Builder => StoredStyle::Builder,
            Style::Builders(rates) => {
                assert!(!rates.is_empty(), "a builders style has a rate for one");
                let start = run(self.rates.len());
                self.rates.extend_from_slice(rates);
                StoredStyle::Builders(start..run(self.rates.len()))
            }
        };
        let near = run(self.rules.len());
        self.rules.extend(build.near);
        let away = run(self.rules.len());
        self.rules.extend(build.away);
        self.specs.push(Stored {
            action,
            unit_type: build.unit_type,
            form: build.form,
            start: build.start,
            style,
            start_life: build.start_life,
            refund: build.refund,
            near: near..away,
            away: away..run(self.rules.len()),
        });
    }

    /// The build in `slot` of `slots`, as `book` holds its action; `None` for a slot that holds
    /// no build, or one not learned. It borrows the books alone, so a placement's test may run beside it.
    pub(crate) fn building(
        &self,
        book: &ActionBook,
        slots: &ActionSlots,
        slot: u8,
    ) -> Option<Building<'_>> {
        let spec = self.of(slots.slot(slot)?.action?)?;
        Some(Building {
            spec,
            range: slots.meters_in(book, slot)?,
        })
    }

    /// The build of `action`; `None` for an action that builds nothing.
    pub fn of(&self, action: ActionId) -> Option<BuildSpec<'_>> {
        let at = self
            .specs
            .binary_search_by_key(&action, |spec| spec.action)
            .ok()?;
        let spec = &self.specs[at];
        let range = |run: &Range<u32>| run.start as usize..run.end as usize;
        let style = match &spec.style {
            StoredStyle::Alone => Style::Alone,
            StoredStyle::Builder => Style::Builder,
            StoredStyle::Builders(rates) => Style::Builders(&self.rates[range(rates)]),
        };
        Some(BuildSpec {
            unit_type: spec.unit_type,
            form: spec.form,
            start: spec.start,
            style,
            start_life: spec.start_life,
            refund: spec.refund,
            near: &self.rules[range(&spec.near)],
            away: &self.rules[range(&spec.away)],
        })
    }
}

impl BuildSpec<'_> {
    /// The building's box, before its angle turns it.
    pub const fn form(self) -> BodyForm {
        self.form
    }

    /// The progress a tick adds to a site with `builders` builders that build it: 1 for `alone`;
    /// for `builder`, 1 with one or more, else 0; for `builders`, the table's entry for the
    /// count, the last for a count past it, and 0 for none.
    pub(crate) fn rate(self, builders: usize) -> Num {
        match (self.style, builders) {
            (Style::Builder | Style::Builders(_), 0) => Num::ZERO,
            (Style::Alone | Style::Builder, _) => Num::ONE,
            (Style::Builders(rates), count) => rates[(count - 1).min(rates.len() - 1)],
        }
    }
}

impl Book for BuildBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}

impl BuildBook {
    /// The build of each action that constructs: its style, the body of its building, a type of its
    /// package that stands, its life and refund at the start, and its placement rules; an error for
    /// a name the match does not have, a building of no body (04-capabilities/production.md, A
    /// building may stand on a circle), or a start of no life for a building that lives.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let tags = cx.fact::<UnitTypes>()?.tags();
        let bodies = cx.book::<ById<UnitType, BodyForm>>()?;
        let mut book = Self::default();
        for action in LoadedAction::all(cx)? {
            let Some(data) = action.section::<BuildData>() else {
                continue;
            };
            let check = |rule: &PlacementRule| {
                Ok(PlacementCheck {
                    filter: Filter::resolve(&rule.filter, tags)?,
                    distance: rule.distance.get(),
                })
            };
            let rules = |of: fn(&PlacementData) -> &[PlacementRule]| {
                let placement = data.placement.as_ref();
                placement.map_or_else(
                    || Ok(Vec::new()),
                    |placement| of(placement).iter().map(check).collect(),
                )
            };
            let style = match &data.construct {
                ConstructData::Alone => Style::Alone,
                ConstructData::Builder => Style::Builder,
                ConstructData::Builders(rates) => Style::Builders(rates),
            };
            let building = &data.unit_type;
            let scope = TypeScope::of_package(action.package);
            let unit_type = cx.unit_type(scope, building.as_str());
            let unit_type = unit_type.filter(|&unit_type| cx.type_of(unit_type).stands());
            let unit_type = unit_type
                .ok_or_else(|| action.unknown(UnknownName::new(UnitType::NAMES, building)))?;
            let form = bodies.get(unit_type).copied();
            let form = form.filter(|form| form.shape().circle_radius() != Some(Num::ZERO));
            let form = form.ok_or_else(|| action.broken(ProductionDataError::NoBody))?;
            let alive = book_builders::lives(cx, unit_type)?;
            let start_life = data.start_life;
            let starts = start_life.is_some_and(|share| !share.is_zero());
            if (alive && !starts) || start_life.is_some_and(Share::is_zero) {
                return Err(action.broken(ProductionDataError::StartLife));
            }
            let near = rules(|placement| &placement.near).map_err(|name| action.unknown(name))?;
            let away = rules(|placement| &placement.away).map_err(|name| action.unknown(name))?;
            book.push(
                action.action,
                NewBuild {
                    unit_type,
                    form,
                    start: data.start,
                    style,
                    start_life,
                    refund: data.cancel_refund.unwrap_or(Share::ALL),
                    near,
                    away,
                },
            );
        }
        Ok(book)
    }
}

#[cfg(test)]
mod tests {
    use campfire_units::{BodyShape, Layer};

    use super::*;

    #[test]
    fn each_style_gives_its_rate_for_each_count_of_builders() {
        // Rates 1, 1.5 and 2 for one, two and three builders, the last for any count past it.
        let rates = [Num::ONE, Num::ONE + Num::HALF, Num::int(2)];
        let mut specs = BuildBook::default();
        for (at, style) in [Style::Alone, Style::Builder, Style::Builders(&rates)]
            .into_iter()
            .enumerate()
        {
            let build = NewBuild {
                unit_type: UnitType::new(0),
                form: BodyForm::new(
                    BodyShape::box_sized([Num::ONE, Num::ONE]).unwrap(),
                    Layer::FIRST,
                ),
                start: BuildStart::Arrival,
                style,
                start_life: None,
                refund: Share::ALL,
                near: Vec::new(),
                away: Vec::new(),
            };
            specs.push(ActionId::new(u32::try_from(at).unwrap() * 2), build);
        }
        let rate = |action, builders| specs.of(ActionId::new(action)).unwrap().rate(builders);
        let counts = [0, 1, 2, 3, 7];
        assert_eq!(counts.map(|count| rate(0, count)), [Num::ONE; 5]);
        assert_eq!(
            counts.map(|count| rate(2, count)),
            [Num::ZERO, Num::ONE, Num::ONE, Num::ONE, Num::ONE]
        );
        assert_eq!(
            counts.map(|count| rate(4, count)),
            [Num::ZERO, rates[0], rates[1], rates[2], rates[2]]
        );
        assert!(specs.of(ActionId::new(1)).is_none());
    }
}
