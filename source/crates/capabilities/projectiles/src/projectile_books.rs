use campfire_actions::{ActionsData, DeliveryData, Flights, ListHook, Targeting};
use campfire_declarations::{BookCx, BookError, BookPlace, ByType, Section};
use campfire_math::Num;
use campfire_script::ScriptRole;
use campfire_stats::MoveSpeedCap;
use campfire_units::{ScriptBook, UnitTypes};

use crate::Projectiles;
use crate::error::{ProjectileDeliveryError, ProjectileError};
use crate::projectile_data::{ProjectileData, ProjectileHits};
use crate::projectile_spec::ProjectileSpec;

impl Projectiles {
    /// The rule of every projectile type: a homing one is faster than the mode's move speed cap,
    /// and one that hits nothing neither homes, stops on a hit, hits once a cast, nor is wide on
    /// a line, as a curve's width meets its target.
    pub(crate) fn rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        let cap = cx.given::<MoveSpeedCap>().map_or(Num::MAX, |cap| cap.0);
        for unit in cx.types() {
            let Some(projectile) = unit.data.section::<ProjectileData>() else {
                continue;
            };
            let fail = |problem| {
                let at = BookPlace::UnitType(unit.place());
                Err(BookError::rule(unit.package, at, problem))
            };
            if projectile.gravity.is_some() {
                return fail(ProjectileError::Gravity);
            }
            if projectile.homing && projectile.speed <= cap {
                return fail(ProjectileError::NotFaster);
            }
            let hits_aside = projectile.homing
                || projectile.stop_on_hit
                || projectile.once_per_cast
                || (projectile.width > Num::ZERO && projectile.curve.is_none());
            if projectile.hits == ProjectileHits::Nothing && hits_aside {
                return fail(ProjectileError::HitsNothing);
            }
        }
        Ok(())
    }

    /// Tags each projectile type `projectile`.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn tag_types(cx: &BookCx<'_>, types: &mut UnitTypes) -> Result<(), BookError> {
        for unit in cx.types() {
            if unit.data.section::<ProjectileData>().is_some() {
                types.give_tag(unit.unit_type, Self::TAG.tag());
            }
        }
        Ok(())
    }

    /// The rule of every action's projectile delivery: a unit type of the projectile section,
    /// aimed; a homing one alone and at a unit, a curve alone and at a unit or a point; one that
    /// hits a unit when the action has an `on_hit` list or hook, as a curve always meets what it
    /// aims at.
    pub(crate) fn deliveries_rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        let scripts = cx.given::<ScriptBook>();
        for (package, source) in cx.packages() {
            let Some(actions) = source.content.get::<ActionsData>() else {
                continue;
            };
            for (id, action) in &actions.0 {
                let Some(DeliveryData::Projectile {
                    unit_type, count, ..
                }) = &action.delivery
                else {
                    continue;
                };
                let fail = |problem| {
                    let at = BookPlace::Entry {
                        table: ActionsData::KEY,
                        name: id.clone(),
                    };
                    Err(BookError::rule(package, at, problem))
                };
                // A type the package does not have is the actions' problem.
                let Some(unit) = source.units.get(unit_type) else {
                    continue;
                };
                let Some(projectile) = unit.section::<ProjectileData>() else {
                    return fail(ProjectileDeliveryError::NotProjectile(unit_type.clone()));
                };
                let at_unit = matches!(action.targeting, Targeting::Unit(_));
                let along = action.targeting == Targeting::Direction;
                // A script the package does not hold is the scripts' problem, and defines no hook.
                let script = action
                    .script
                    .as_ref()
                    .and_then(|path| cx.script(package, path).ok());
                let hooks = scripts.map(|scripts| scripts.defines(script, ScriptRole::Action));
                let hooks_hit = hooks.is_some_and(|hooks| hooks.contains(ListHook::OnHit.hook()));
                let problem = if action.targeting == Targeting::None {
                    ProjectileDeliveryError::NoAim
                } else if projectile.homing && (!at_unit || count.get() > 1) {
                    ProjectileDeliveryError::Homing
                } else if projectile.curve.is_some() && (along || count.get() > 1) {
                    ProjectileDeliveryError::Curve
                } else if projectile.hits == ProjectileHits::Nothing
                    && projectile.curve.is_none()
                    && (hooks_hit || !action.on_hit.is_empty())
                {
                    ProjectileDeliveryError::NoHit
                } else {
                    continue;
                };
                return fail(problem);
            }
        }
        Ok(())
    }

    /// How each projectile type flies.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn flights(cx: &BookCx<'_>) -> Result<Flights, BookError> {
        let mut flights = ByType::default();
        for unit in cx.types() {
            if let Some(projectile) = unit.data.section::<ProjectileData>() {
                flights.set(unit.unit_type, projectile.flight());
            }
        }
        Ok(Flights(flights))
    }
}

impl ProjectileSpec {
    /// The spec of each projectile type, its filters resolved against the match's tags; an error
    /// for a tag the match does not have.
    pub fn book(cx: &BookCx<'_>) -> Result<ByType<Self>, BookError> {
        let tags = cx.fact::<UnitTypes>()?.tags();
        let mut book = ByType::default();
        for unit in cx.types() {
            let Some(projectile) = unit.data.section::<ProjectileData>() else {
                continue;
            };
            let spec = Self::of(projectile, tags, cx.rate()).map_err(|name| {
                BookError::unknown(unit.package, BookPlace::UnitType(unit.place()), name)
            })?;
            book.set(unit.unit_type, spec);
        }
        Ok(book)
    }
}
