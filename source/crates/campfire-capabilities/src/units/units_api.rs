use campfire_math::{Num, Vec3};
use campfire_script::rhai::{Dynamic, INT, NativeCallContext};
use campfire_script::{NumError, Raised};
use campfire_sim::Position;

use crate::geometry::shape::Shape;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::script_api::api_owner::ApiOwner;
use crate::scripts::script_api::member_spec::MemberSpec;

/// The script API of positions and vectors: `Pos` with `distance_to`, `within` and
/// `direction_to`, and `Vector` with `rotated_deg`.
#[derive(Debug)]
pub(crate) struct UnitsApi;

impl UnitsApi {
    /// `Pos` and `Vector`.
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let position = |name, signature, description| {
            MemberSpec::method(ApiOwner::Position, name, signature, description)
        };
        let within = position(
            "within",
            &[&["pos", "radius"]],
            "whether `pos` is within `radius` in the map's metric, exactly: the reach rule between two points, which have no bodies",
        );
        api.ty::<Position>("Pos")
            .bind(
                position(
                    "distance_to",
                    &[&["pos"]],
                    "the distance to `pos` in the map's metric",
                ),
                |call: NativeCallContext<'_>, from: Position, to: Position| {
                    Ctx::of_call(&call)
                        .view()
                        .metric()
                        .offset(from, to)
                        .checked_length()
                        .ok_or_else(|| Box::new(Raised::error(NumError::Overflow)))
                },
            )
            .bind(
                within,
                |call: NativeCallContext<'_>, from: Position, to: Position, radius: Num| {
                    UnitsApi::within(&call, from, to, radius)
                },
            )
            .bind(
                within,
                |call: NativeCallContext<'_>, from: Position, to: Position, radius: INT| {
                    UnitsApi::within(&call, from, to, ApiError::num(radius)?)
                },
            )
            .bind(
                position(
                    "direction_to",
                    &[&["pos"]],
                    "the unit vector towards `pos` in the map's metric, `()` for the same point",
                ),
                |call: NativeCallContext<'_>, from: Position, to: Position| {
                    Ctx::of_call(&call)
                        .view()
                        .metric()
                        .offset(from, to)
                        .normalized()
                        .map_or(Dynamic::UNIT, Dynamic::from)
                },
            );
        let vector = |name, signature, description| {
            MemberSpec::method(ApiOwner::Vector, name, signature, description)
        };
        let rotated = vector(
            "rotated_deg",
            &[&["degrees"]],
            "the vector turned by `degrees` about the vertical, counter-clockwise seen from above",
        );
        api.ty::<Vec3>("Vector")
            .bind(rotated, |vector: Vec3, degrees: Num| {
                UnitsApi::rotated(vector, degrees)
            })
            .bind(rotated, |vector: Vec3, degrees: INT| {
                UnitsApi::rotated(vector, ApiError::num(degrees)?)
            });
        let scaled = MemberSpec::operator(
            ApiOwner::Vector,
            "*",
            "the vector scaled by a number, on either side",
        );
        api.bind(
            MemberSpec::operator(ApiOwner::Vector, "+", "the sum of two vectors"),
            |a: Vec3, b: Vec3| UnitsApi::exact(a.checked_add(b)),
        )
        .bind(
            MemberSpec::operator(ApiOwner::Vector, "-", "the difference of two vectors"),
            |a: Vec3, b: Vec3| UnitsApi::exact(a.checked_sub(b)),
        )
        .bind(scaled, |vector: Vec3, factor: Num| {
            UnitsApi::exact(vector.checked_scale(factor))
        })
        .bind(scaled, |factor: Num, vector: Vec3| {
            UnitsApi::exact(vector.checked_scale(factor))
        })
        .bind(scaled, |vector: Vec3, factor: INT| {
            UnitsApi::exact(vector.checked_scale(ApiError::num(factor)?))
        })
        .bind(scaled, |factor: INT, vector: Vec3| {
            UnitsApi::exact(vector.checked_scale(ApiError::num(factor)?))
        });
    }

    /// The vector a checked operation gave; one past what a number holds fails the call.
    fn exact(vector: Option<Vec3>) -> Checked<Vec3> {
        vector.ok_or_else(|| Box::new(Raised::error(NumError::Overflow)))
    }

    /// `vector` turned by `degrees` about the vertical; a turn too large to compute fails the call.
    fn rotated(vector: Vec3, degrees: Num) -> Checked<Vec3> {
        let radians = degrees
            .checked_mul(Num::PI)
            .and_then(|turn| turn.checked_div_int(180))
            .ok_or_else(|| Box::new(Raised::error(NumError::Overflow)))?;
        UnitsApi::exact(vector.checked_rotated_y(radians.sin_cos()))
    }

    /// Whether `to` is within `radius` of `from` in the map's metric, exactly: the reach rule
    /// between two points, which have no bodies.
    fn within(
        call: &NativeCallContext<'_>,
        from: Position,
        to: Position,
        radius: Num,
    ) -> Checked<bool> {
        if radius < Num::ZERO {
            return Err(ApiError::NegativeRadius.fail().into());
        }
        let metric = Ctx::of_call(call).view().metric();
        Ok(metric.reaches(from, Shape::POINT, radius, to, Shape::POINT))
    }
}
