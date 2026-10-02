use campfire_math::{Num, Vec3};
use campfire_script::rhai::{Dynamic, INT, NativeCallContext};
use campfire_script::{NumError, Raised};
use campfire_sim::Position;

use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::script_api::{ApiOwner, MemberSpec};

/// The script API of positions and vectors: `Pos` with `distance_to`, `within` and
/// `direction_to`, and `Vector` with `rotated_deg`.
#[derive(Debug)]
pub(crate) struct PositionApi;

impl PositionApi {
    /// `Pos` and `Vector`.
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let position = |name, signature, description| {
            MemberSpec::method(ApiOwner::Position, name, signature, description)
        };
        let within = position(
            "within",
            "(pos, radius)",
            "whether `pos` is within `radius` in the map's metric, exactly: the reach rule between two points, which have no bodies",
        );
        api.ty::<Position>("Pos")
            .bind(
                position(
                    "distance_to",
                    "(pos)",
                    "the distance to `pos` in the map's metric",
                ),
                |call: NativeCallContext<'_>, from: &mut Position, to: Position| {
                    Ctx::of_call(&call)
                        .view()
                        .metric()
                        .offset(*from, to)
                        .checked_length()
                        .ok_or_else(|| Box::new(Raised::error(NumError::Overflow)))
                },
            )
            .bind(
                within,
                |call: NativeCallContext<'_>, from: &mut Position, to: Position, radius: Num| {
                    PositionApi::within(&call, *from, to, radius)
                },
            )
            .bind(
                within,
                |call: NativeCallContext<'_>, from: &mut Position, to: Position, radius: INT| {
                    PositionApi::within(&call, *from, to, ApiError::num(radius)?)
                },
            )
            .bind(
                position(
                    "direction_to",
                    "(pos)",
                    "the unit vector towards `pos` in the map's metric, `()` for the same point",
                ),
                |call: NativeCallContext<'_>, from: &mut Position, to: Position| {
                    Ctx::of_call(&call)
                        .view()
                        .metric()
                        .offset(*from, to)
                        .normalized()
                        .map_or(Dynamic::UNIT, Dynamic::from)
                },
            );
        let vector = |name, signature, description| {
            MemberSpec::method(ApiOwner::Vector, name, signature, description)
        };
        let rotated = vector(
            "rotated_deg",
            "(degrees)",
            "the vector turned by `degrees` about the vertical, counter-clockwise seen from above",
        );
        api.ty::<Vec3>("Vector")
            .bind(rotated, |vector: &mut Vec3, degrees: Num| {
                PositionApi::rotated(*vector, degrees)
            })
            .bind(rotated, |vector: &mut Vec3, degrees: INT| {
                PositionApi::rotated(*vector, ApiError::num(degrees)?)
            });
        api.plan(MemberSpec::operator(
            ApiOwner::Vector,
            "+",
            "the sum of two vectors",
        ))
        .plan(MemberSpec::operator(
            ApiOwner::Vector,
            "-",
            "the difference of two vectors",
        ))
        .plan(MemberSpec::operator(
            ApiOwner::Vector,
            "*",
            "the vector scaled by a number",
        ));
    }

    /// `vector` turned by `degrees` about the vertical; a turn too large to compute fails the call.
    fn rotated(vector: Vec3, degrees: Num) -> Checked<Vec3> {
        let radians = degrees
            .checked_mul(Num::PI)
            .and_then(|turn| turn.checked_div_int(180))
            .ok_or_else(|| Box::new(Raised::error(NumError::Overflow)))?;
        vector
            .checked_rotated_y(radians.sin_cos())
            .ok_or_else(|| Box::new(Raised::error(NumError::Overflow)))
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
        Ok(metric.reaches(from, Num::ZERO, radius, to, Num::ZERO))
    }
}
