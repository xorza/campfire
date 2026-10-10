use std::collections::BTreeMap;
use std::f32::consts::PI;

use campfire_math::Num;
use campfire_sim::Position;

use crate::zero_hour::chunk_reader::ChunkReader;
use crate::zero_hour::error::MapError;

/// An object of a map's `ObjectsList`, as `WorldHeightMap::ParseObjectData` reads it: its
/// position, `z` above the ground, its angle in radians, its flags, its template's name, and
/// whether its properties make it a waypoint.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MapObject {
    pub(crate) pos: [f32; 3],
    pub(crate) angle: f32,
    pub(crate) flags: u32,
    pub(crate) template: Vec<u8>,
    pub(crate) waypoint: Option<Waypoint>,
}

/// A waypoint's id, and its name, empty when its properties give none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Waypoint {
    pub(crate) id: i32,
    pub(crate) name: Vec<u8>,
}

/// A dictionary value the import reads, or one it skips.
#[derive(Debug)]
enum DictValue<'a> {
    Int(i32),
    Text(&'a [u8]),
    Other,
}

/// An object placed on the ground plane: its `[x, z]`, its height above the ground, and its angle
/// in degrees, counter-clockwise seen from above.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Placement {
    pub(crate) ground: [Num; 2],
    pub(crate) height: Num,
    pub(crate) degrees: Num,
}

/// The flags of a road's or a bridge's ends, which the terrain draws and no object spawns from
/// (`FLAG_ROAD_FLAGS` and `FLAG_BRIDGE_FLAGS`).
const ROAD_OR_BRIDGE: u32 = 0x36;

/// The heights past which the game drops an object as it reads it: −100 cells and 2,550 height
/// steps, each exact in `f32`.
const LOWEST: f32 = -1000.0;
const HIGHEST: f32 = 1593.75;

/// 180/π · 2⁹⁶, rounded to nearest: 57.29577951308232087679815481410…, within 2⁻⁹⁷ of exact.
const DEGREES_PER_RADIAN: u128 = 0x0039_4bb8_34c7_83ef_70c2_a5d4_dfd0;

/// The fraction bits of `DEGREES_PER_RADIAN`.
const DEGREES_BITS: i32 = 96;

impl MapObject {
    /// The object whose fields `body` holds, past `ParseObjectData`'s version 3; its
    /// dictionary's keys named by `names`.
    pub(crate) fn read(
        body: &mut ChunkReader<'_>,
        names: &BTreeMap<u32, &[u8]>,
    ) -> Result<MapObject, MapError> {
        let pos = [body.f32()?, body.f32()?, body.f32()?];
        let angle = body.f32()?;
        let flags = body.u32()?;
        let template = body.text()?.to_vec();
        // A key set twice keeps its last value and type, as the game's `Dict` does; the object is
        // a waypoint when its `waypointID` is an integer.
        let mut id = None;
        let mut name = None;
        for _ in 0..body.u16()? {
            let key_and_type = body.i32()?;
            let key_id = (key_and_type >> 8).cast_unsigned();
            let key = *names.get(&key_id).ok_or(MapError::UnknownName(key_id))?;
            let value = match key_and_type.to_le_bytes()[0] {
                0 => body.take(1).map(|_| DictValue::Other)?,
                1 => DictValue::Int(body.i32()?),
                2 => body.take(4).map(|_| DictValue::Other)?,
                3 => DictValue::Text(body.text()?),
                4 => {
                    let len = body.u16()?;
                    body.take(usize::from(len) * 2).map(|_| DictValue::Other)?
                }
                other => return Err(MapError::DictType(other)),
            };
            match (key, value) {
                (b"waypointID", DictValue::Int(value)) => id = Some(value),
                (b"waypointID", _) => id = None,
                (b"waypointName", DictValue::Text(value)) => name = Some(value),
                (b"waypointName", _) => name = None,
                _ => {}
            }
        }
        Ok(MapObject {
            pos,
            angle,
            flags,
            template,
            waypoint: id.map(|id| Waypoint {
                id,
                name: name.unwrap_or_default().to_vec(),
            }),
        })
    }

    /// Whether the game keeps the object as it reads the map: its height is not past the
    /// bounds it checks, which a NaN never is.
    pub(crate) fn kept(&self) -> bool {
        let z = self.pos[2];
        !(z < LOWEST || z > HIGHEST)
    }

    /// Whether it is a road's or a bridge's end, which the game draws as terrain.
    pub(crate) const fn road_or_bridge(&self) -> bool {
        self.flags & ROAD_OR_BRIDGE != 0
    }

    /// Where it stands on the engine's ground plane: the original's `x` east and `y` north become
    /// `x` and `−z`, which keeps the axes right-handed with `y` up. `None` past the world's bound.
    pub(crate) fn ground(&self) -> Option<[Num; 2]> {
        let [x, y, _] = self.pos.map(MapObject::num);
        let ground = [x?, y?.checked_neg()?];
        let bound = Position::BOUND.to_bits();
        ground
            .iter()
            .all(|value| value.to_bits().unsigned_abs() <= bound.unsigned_abs())
            .then_some(ground)
    }

    /// How it stands as a unit: its ground point, the original's `z` up as its height, and its
    /// angle, turned from `x` towards `y`, which keeps its sign, as a turn from `x` towards `−z`
    /// is counter-clockwise seen from above. `None` past the world's bound, or for an angle the
    /// game's own normalization never ends on. A waypoint takes its ground point alone, as the
    /// game reads neither its height nor its angle.
    pub(crate) fn placement(&self) -> Option<Placement> {
        Some(Placement {
            ground: self.ground()?,
            height: MapObject::num(self.pos[2])?,
            degrees: MapObject::degrees(MapObject::normalized(self.angle)?)?,
        })
    }

    /// `angle` as the game's `normalizeAngle` gives it, in `f32` steps: a NaN as 0, and any other
    /// in (−π, π] by whole turns; `None` for one no turn moves.
    fn normalized(angle: f32) -> Option<f32> {
        // The source's `PI`, 3.14159265359f, is the `f32` nearest π, as `PI` is.
        let turn = 2.0 * PI;
        if angle.is_nan() {
            return Some(0.0);
        }
        let mut angle = angle;
        while angle > PI {
            let next = angle - turn;
            (next.to_bits() != angle.to_bits()).then_some(())?;
            angle = next;
        }
        while angle <= -PI {
            let next = angle + turn;
            (next.to_bits() != angle.to_bits()).then_some(())?;
            angle = next;
        }
        Some(angle)
    }

    /// `radians`, finite and within ±π, in degrees, rounded once to the nearest `Num`. The
    /// product with `DEGREES_PER_RADIAN` is within half the mantissa, below 2²³ units, of exact;
    /// the true value is irrational for any angle but 0, so it is never a tie, and `None` only
    /// when it lies within that error of one.
    fn degrees(radians: f32) -> Option<Num> {
        let bits = radians.to_bits();
        let exponent = i32::try_from((bits >> 23) & 0xFF).expect("eight bits");
        let fraction = bits & 0x7F_FFFF;
        let (mantissa, power) = if exponent == 0 {
            (fraction, -149)
        } else {
            (fraction | 0x80_0000, exponent - 150)
        };
        // Below 2⁻³¹ radians, the degrees are below 57.3 · 2⁻³¹, under half a `Num`'s step of
        // 2⁻²⁴.
        if mantissa == 0 || power <= -55 {
            return Some(Num::ZERO);
        }
        let product = u128::from(mantissa) * DEGREES_PER_RADIAN;
        let shift = u32::try_from(DEGREES_BITS - Num::FRAC_BITS.cast_signed() - power)
            .expect("a normalized angle's exponent is at most −22, so the shift is at least 94");
        let half = 1_u128 << (shift - 1);
        let rest = product & ((1 << shift) - 1);
        if rest.abs_diff(half) <= 1 << 23 {
            return None;
        }
        let rounded = (product >> shift) + u128::from(rest > half);
        let magnitude = i64::try_from(rounded).expect("degrees within ±180");
        Some(Num::from_bits(if radians < 0.0 {
            -magnitude
        } else {
            magnitude
        }))
    }

    /// `value` as a `Num`, exact for any value of at least 2⁻¹ and rounded once to nearest,
    /// ties to even, below; `None` for one that is not finite or past what a `Num` holds.
    fn num(value: f32) -> Option<Num> {
        if !value.is_finite() {
            return None;
        }
        let bits = value.to_bits();
        let exponent = i32::try_from((bits >> 23) & 0xFF).expect("eight bits");
        let fraction = u64::from(bits & 0x7F_FFFF);
        let (mantissa, power) = if exponent == 0 {
            (fraction, -149)
        } else {
            (fraction | 0x80_0000, exponent - 150)
        };
        let scaled = power + Num::FRAC_BITS.cast_signed();
        let magnitude = if scaled >= 0 {
            mantissa
                .checked_shl(scaled.cast_unsigned())
                .filter(|shifted| shifted >> scaled.cast_unsigned() == mantissa)?
        } else if scaled < -63 {
            0
        } else {
            let shift = scaled.unsigned_abs();
            let half = 1 << (shift - 1);
            let rest = mantissa & ((1 << shift) - 1);
            let floor = mantissa >> shift;
            floor + u64::from(rest > half || (rest == half && floor & 1 == 1))
        };
        let magnitude = i64::try_from(magnitude).ok()?;
        Some(Num::from_bits(if value < 0.0 {
            -magnitude
        } else {
            magnitude
        }))
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::zero_hour::chunk_reader::internals::ChunkWriter;

    /// An object's body as the game writes it: its position, angle, flags and template, and a
    /// dictionary of its waypoint's id and name when it has one, beside an initial health.
    pub(crate) fn object(
        writer: &mut ChunkWriter,
        pos: [f32; 3],
        angle: f32,
        flags: u32,
        template: &str,
        waypoint: Option<(i32, &str)>,
    ) -> Vec<u8> {
        let mut entries = vec![writer.entry("objectInitialHealth", 1, &100_i32.to_le_bytes())];
        if let Some((id, name)) = waypoint {
            entries.push(writer.entry("waypointID", 1, &id.to_le_bytes()));
            entries.push(writer.entry("waypointName", 3, &ChunkWriter::text(name.as_bytes())));
        }
        body(pos, angle, flags, template, &entries)
    }

    /// An object's body of these fields and dictionary `entries`.
    pub(crate) fn body(
        pos: [f32; 3],
        angle: f32,
        flags: u32,
        template: &str,
        entries: &[Vec<u8>],
    ) -> Vec<u8> {
        let mut body = [pos[0], pos[1], pos[2], angle]
            .map(f32::to_le_bytes)
            .concat();
        body.extend(flags.to_le_bytes());
        body.extend(ChunkWriter::text(template.as_bytes()));
        body.extend(u16::try_from(entries.len()).unwrap().to_le_bytes());
        body.extend(entries.concat());
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::chunk_reader::internals::ChunkWriter;
    use crate::zero_hour::map_object::internals::body;

    #[test]
    fn an_angle_turns_to_degrees_as_the_game_normalizes_it() {
        // 180/π · 2²⁴ = 961,263,668.78…; π/2 in `f32` is 1.57079637…, past the true 1.57079632…,
        // so 90.0000025… degrees, 1,509,949,482.02… in steps of 2⁻²⁴.
        let degrees = |radians: f32| MapObject::degrees(radians).unwrap().to_bits();
        assert_eq!(degrees(1.0), 961_263_669);
        assert_eq!(degrees(-1.0), -961_263_669);
        assert_eq!(degrees(PI / 2.0), 1_509_949_482);
        assert_eq!(degrees(0.0), 0);
        // 2⁻³⁰ radians is 57.3 · 2⁻³⁰ degrees, 0.89 of a step; 2⁻³¹ is 0.45, rounded to 0.
        assert_eq!(degrees(2.0_f32.powi(-30)), 1);
        assert_eq!(degrees(2.0_f32.powi(-31)), 0);
        assert_eq!(degrees(f32::from_bits(1)), 0);

        // 4.729842 is past π by less than a turn: one step of 2π in `f32`, as the game takes it.
        let past = 4.729_842_f32;
        assert_eq!(MapObject::normalized(past), Some(past - 2.0 * PI));
        assert_eq!(MapObject::normalized(-PI), Some(PI));
        assert_eq!(MapObject::normalized(PI), Some(PI));
        assert_eq!(MapObject::normalized(f32::NAN), Some(0.0));
        assert_eq!(MapObject::normalized(f32::INFINITY), None);
        assert_eq!(MapObject::normalized(1e30), None);
    }

    #[test]
    fn an_object_keeps_each_keys_last_value_and_its_height_unless_past_the_games_bounds() {
        let mut writer = ChunkWriter::default();
        let read = |writer: &ChunkWriter, entries: &[Vec<u8>]| {
            let bytes = body([1.0, 2.0, 0.0], 0.0, 0, "Rock", entries);
            MapObject::read(&mut ChunkReader::new(&bytes), &writer.table()).unwrap()
        };
        let id = writer.entry("waypointID", 1, &7_i32.to_le_bytes());
        let name = writer.entry("waypointName", 3, &ChunkWriter::text(b"Start"));
        let as_real = writer.entry("waypointID", 2, &1.0_f32.to_le_bytes());
        let waypoint = read(&writer, &[id.clone(), name.clone()]);
        assert_eq!(
            waypoint.waypoint,
            Some(Waypoint {
                id: 7,
                name: b"Start".to_vec()
            })
        );
        // An id set again as a real makes no waypoint, as the game's `Dict` keeps the last type.
        assert_eq!(read(&writer, &[id, as_real, name]).waypoint, None);

        // The game drops an object past −1,000 or 1,593.75, not one at either, nor a NaN height,
        // whose placement the engine cannot hold.
        let at = |z: f32| MapObject {
            pos: [0.0, 0.0, z],
            ..waypoint.clone()
        };
        assert!(at(-1000.0).kept() && at(1593.75).kept() && at(f32::NAN).kept());
        assert!(!at(-1000.1).kept() && !at(1594.0).kept());
        assert_eq!(at(f32::NAN).placement(), None);
    }

    #[test]
    fn a_position_converts_exactly_or_rounds_once_below_a_half() {
        let num = |value: f32| MapObject::num(value).map(Num::to_bits);
        // 1234.5677 in `f32` is 1234.5677490234375, 20,712,609,792 steps of 2⁻²⁴.
        assert_eq!(num(1234.5677), Some(20_712_609_792));
        assert_eq!(num(-0.625), Some(-10_485_760));
        // 2⁻²⁵ is half a step and rounds to the even 0; 3 · 2⁻²⁵ is a step and a half, and
        // rounds to the even 2.
        assert_eq!(num(2.0_f32.powi(-25)), Some(0));
        assert_eq!(num(3.0 * 2.0_f32.powi(-25)), Some(2));
        assert_eq!(num(5.0 * 2.0_f32.powi(-26)), Some(1));
        assert_eq!(num(f32::MIN_POSITIVE), Some(0));
        assert_eq!(num(f32::NAN), None);
        assert_eq!(num(1e12), None);

        // The original's north is the engine's −z, and its angle keeps its sign.
        let object = MapObject {
            pos: [30.0, 20.0, -4.5],
            angle: PI / 2.0,
            flags: 0,
            template: b"Rock".to_vec(),
            waypoint: None,
        };
        let placement = object.placement().unwrap();
        assert_eq!(placement.ground, [Num::int(30), Num::int(-20)]);
        assert_eq!(placement.height, Num::int(-9) / Num::int(2));
        assert_eq!(placement.degrees.to_bits(), 1_509_949_482);
        let far = MapObject {
            pos: [2_000_000.0, 0.0, 0.0],
            ..object
        };
        assert_eq!(far.placement(), None);
    }
}
