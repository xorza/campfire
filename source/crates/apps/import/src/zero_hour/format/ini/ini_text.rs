use std::iter;
use std::ops::Range;

use campfire_client_files::Spread;

use crate::zero_hour::format::ini::error::IniError;

/// An INI file's text, as the game reads it: bytes as Latin-1 chars, each kept as one, and the
/// same folded to ASCII lowercase, at the same offsets, as the game matches a keyword.
#[derive(Debug)]
pub(crate) struct IniText {
    text: String,
    lower: String,
}

/// An INI file's lines that hold a token, read once: every token in one list, and each line its
/// run of them, so a line allocates nothing.
#[derive(Debug)]
pub(crate) struct IniLines<'a> {
    tokens: Vec<&'a str>,
    lines: Vec<LineTokens<'a>>,
}

/// A line's number, its run in its file's tokens, and its first token folded to lowercase.
#[derive(Debug, Clone)]
struct LineTokens<'a> {
    number: usize,
    tokens: Range<usize>,
    word: &'a str,
}

/// A field's list of flags, as `INI::parseBitString32` and `KindOf` read it: `NONE`, which clears
/// every flag and ends the list; names, which clear every flag and set those; or names of `+` and
/// `-`, which set and clear those and keep the others.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FlagList<'a> {
    Clear,
    Only(Vec<&'a str>),
    Change(Vec<FlagChange<'a>>),
}

/// A set of flags a field's list changes, as `IniLine::set_flags` changes it: it empties, and it
/// sets or clears one flag by its name, of either case, false for a name it refuses.
pub(crate) trait FlagSet {
    fn clear(&mut self);
    fn set(&mut self, name: &str, on: bool) -> bool;
}

/// A flag a list of `+` and `-` sets, or clears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FlagChange<'a> {
    pub(crate) name: &'a str,
    pub(crate) set: bool,
}

/// A block at an INI file's top, as the game's `INI::load` reads one: its first line, which
/// names its kind and, but for a block of one instance, its name, and its lines up to its
/// `End`.
#[derive(Debug)]
pub(crate) struct IniBlock<'a> {
    pub(crate) head: IniLine<'a>,
    pub(crate) fields: Vec<IniLine<'a>>,
}

/// Which blocks a file's top holds: blocks of one kind alone, so any other line there is an
/// error, or blocks of any kind, as a file of several kinds holds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockKinds<'k> {
    /// The kind, folded to lowercase.
    One(&'k str),
    Any,
}

/// A line of an INI file that holds a token: its number, from 1, and its tokens, as the game's
/// reader splits them: the text before a `;`, cut at every `=` and every char up to a space.
#[derive(Debug, Clone, Copy)]
pub(crate) struct IniLine<'a> {
    pub(crate) number: usize,
    pub(crate) tokens: &'a [&'a str],
    word: &'a str,
}

impl IniText {
    pub(crate) fn of(bytes: &[u8]) -> Self {
        let text: String = bytes.iter().map(|&byte| char::from(byte)).collect();
        let lower = text.to_ascii_lowercase();
        Self { text, lower }
    }

    /// Its lines that hold a token, in order. The lowercase text splits as the text does, as
    /// folding changes no byte the split reads.
    pub(crate) fn lines(&self) -> IniLines<'_> {
        let mut tokens = Vec::new();
        let mut lines = Vec::new();
        let split = Self::split(&self.text).zip(Self::split(&self.lower));
        for (at, (raw, lower)) in split.enumerate() {
            let start = tokens.len();
            tokens.extend(raw);
            let mut lower = lower;
            if let Some(word) = lower.next() {
                lines.push(LineTokens {
                    number: at + 1,
                    tokens: start..tokens.len(),
                    word,
                });
            }
        }
        IniLines { tokens, lines }
    }

    /// The tokens of each line of `text`, as the game's reader splits them.
    fn split(text: &str) -> impl Iterator<Item = impl Iterator<Item = &str>> {
        text.split('\n').map(|raw| {
            let content = raw.split(';').next().unwrap_or_default();
            content
                .split(|c: char| c <= ' ' || c == '=')
                .filter(|token| !token.is_empty())
        })
    }
}

impl IniLines<'_> {
    /// Its lines, in order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = IniLine<'_>> {
        self.lines.iter().map(|line| IniLine {
            number: line.number,
            tokens: &self.tokens[line.tokens.clone()],
            word: line.word,
        })
    }

    /// Its blocks of `kinds`, in order, each up to its next line of `End`, until the first
    /// error: an `End` at the top, a line there of a kind `kinds` does not hold, or a block the
    /// file ends inside.
    pub(crate) fn blocks(
        &self,
        kinds: BlockKinds<'_>,
    ) -> impl Iterator<Item = Result<IniBlock<'_>, IniError>> {
        let mut lines = self.iter();
        let mut failed = false;
        iter::from_fn(move || {
            if failed {
                return None;
            }
            let block = block(lines.next()?, &mut lines, kinds);
            failed = block.is_err();
            Some(block)
        })
    }
}

impl IniBlock<'_> {
    /// Its name, its first line's second token.
    pub(crate) fn name(&self) -> Result<&str, IniError> {
        self.head.tokens.get(1).copied().ok_or(IniError::NoName {
            line: self.head.number,
        })
    }
}

impl<'a> IniLine<'a> {
    /// Its first token, folded to lowercase, as the game matches a keyword.
    pub(crate) const fn word(&self) -> &'a str {
        self.word
    }

    /// The value of a field of yes or no, of either case, as `INI::scanBool` reads it.
    pub(crate) fn yes(&self) -> Result<bool, IniError> {
        let value = self.value()?;
        if value.eq_ignore_ascii_case("yes") {
            Ok(true)
        } else if value.eq_ignore_ascii_case("no") {
            Ok(false)
        } else {
            Err(IniError::Bool { line: self.number })
        }
    }

    /// The value of a field of a whole number: its longest prefix of one, as `sscanf`'s `%d`
    /// reads it, the rest of the token ignored.
    pub(crate) fn int(&self) -> Result<i32, IniError> {
        IniLine::int_of(self.value()?, self.number)
    }

    /// `value`'s longest prefix of a whole number, as `int` reads a field's, of the line `line`.
    fn int_of(value: &str, line: usize) -> Result<i32, IniError> {
        let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
        let end = value.len() - digits.len() + IniLine::digits(digits);
        value[..end]
            .parse()
            .map_err(|error| IniError::Number { line, error })
    }

    /// The value of a field of a real number: its longest prefix of one, as `sscanf`'s `%f` and
    /// the game's `from_chars` read it, the rest of the token ignored, so a token of `40v.0` is
    /// 40; or the whole token, for one of infinity or of no number.
    pub(crate) fn real(&self) -> Result<f32, IniError> {
        IniLine::real_of(self.value()?, self.number)
    }

    /// The whole number of its token at `at`, as `int` reads a field's value.
    pub(crate) fn int_at(&self, at: usize) -> Result<i32, IniError> {
        let token = self
            .tokens
            .get(at)
            .ok_or(IniError::NoValue { line: self.number })?;
        IniLine::int_of(token, self.number)
    }

    /// The real number of its token at `at`, as `real` reads a field's value.
    pub(crate) fn real_at(&self, at: usize) -> Result<f32, IniError> {
        let token = self
            .tokens
            .get(at)
            .ok_or(IniError::NoValue { line: self.number })?;
        IniLine::real_of(token, self.number)
    }

    /// The value of a field of a coordinate, as `INI::parseCoord3D` reads it: each of `labels` in
    /// order, of either case, then its real number, its tokens cut at every `:` as well, as
    /// `INI::getNextSubToken` cuts them, so `X:-18.0` and `X: -18.0` are alike.
    pub(crate) fn coord<const N: usize>(&self, labels: [&str; N]) -> Result<[f32; N], IniError> {
        self.labelled(labels, IniLine::real_of)
    }

    /// The value of a field of whole numbers by label, as `INI::parseRGBColor` reads `R:100
    /// G:114 B:245`: the labels as `coord` reads them, each number as `scanInt` reads it.
    pub(crate) fn int_coord<const N: usize>(
        &self,
        labels: [&str; N],
    ) -> Result<[i32; N], IniError> {
        self.labelled(labels, IniLine::int_of)
    }

    /// Each of `labels` in order, of either case, then its value, read by `read`, its tokens
    /// cut at every `:` as well.
    fn labelled<T: Copy + Default, const N: usize>(
        &self,
        labels: [&str; N],
        read: fn(&str, usize) -> Result<T, IniError>,
    ) -> Result<[T; N], IniError> {
        let mut parts = self.tokens[1..]
            .iter()
            .flat_map(|token| token.split(':'))
            .filter(|part| !part.is_empty());
        let mut values = [T::default(); N];
        for (label, at) in labels.iter().zip(&mut values) {
            let labelled = parts
                .next()
                .is_some_and(|part| part.eq_ignore_ascii_case(label));
            let value = parts.next().filter(|_| labelled);
            *at = read(
                value.ok_or(IniError::Coord { line: self.number })?,
                self.number,
            )?;
        }
        Ok(values)
    }

    /// `value` as a real number, as `real` reads a field's, of the line `line`.
    fn real_of(value: &str, line: usize) -> Result<f32, IniError> {
        let real = value
            .parse()
            .or_else(|_| value[..IniLine::real_prefix(value)].parse());
        real.map_err(|error| IniError::Real { line, error })
    }

    /// The length of `text`'s longest prefix of a decimal real: a sign, digits with a point
    /// among them or after them, and an exponent of digits.
    fn real_prefix(text: &str) -> usize {
        let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
        let mut at = text.len() - unsigned.len();
        let whole = IniLine::digits(&text[at..]);
        at += whole;
        let mut fraction = 0;
        if let Some(rest) = text[at..].strip_prefix('.') {
            fraction = IniLine::digits(rest);
            at += 1 + fraction;
        }
        if whole + fraction == 0 {
            return 0;
        }
        if let Some(rest) = text[at..].strip_prefix(['e', 'E']) {
            let exponent = rest.strip_prefix(['+', '-']).unwrap_or(rest);
            let digits = IniLine::digits(exponent);
            if digits > 0 {
                at += 1 + rest.len() - exponent.len() + digits;
            }
        }
        at
    }

    /// The count of ASCII digits `text` starts with.
    fn digits(text: &str) -> usize {
        text.bytes().take_while(u8::is_ascii_digit).count()
    }

    /// The list of flags its tokens from the second give; an error for a list that mixes names
    /// with names of `+` or `-`, or `NONE` with either.
    pub(crate) fn flags(&self) -> Result<FlagList<'_>, IniError> {
        let mixed = IniError::MixedFlags { line: self.number };
        let mut only = Vec::new();
        let mut changes = Vec::new();
        for &flag in &self.tokens[1..] {
            if flag.eq_ignore_ascii_case("none") {
                if !only.is_empty() || !changes.is_empty() {
                    return Err(mixed);
                }
                return Ok(FlagList::Clear);
            }
            let change = match flag.split_at_checked(1) {
                Some(("+", name)) => Some(FlagChange { name, set: true }),
                Some(("-", name)) => Some(FlagChange { name, set: false }),
                _ => None,
            };
            match change {
                Some(_) if !only.is_empty() => return Err(mixed),
                Some(change) => changes.push(change),
                None if !changes.is_empty() => return Err(mixed),
                None => only.push(flag),
            }
        }
        Ok(if only.is_empty() {
            FlagList::Change(changes)
        } else {
            FlagList::Only(only)
        })
    }

    /// Changes `flags` by its list, as the game's reader does: `NONE` empties them, a list of
    /// names empties them and sets those, and names of `+` and `-` set and clear those and keep
    /// the others; an error for a list `flags` refuses a name of.
    pub(crate) fn set_flags(&self, flags: &mut impl FlagSet) -> Result<(), IniError> {
        let taken = match self.flags()? {
            FlagList::Clear => {
                flags.clear();
                true
            }
            FlagList::Only(names) => {
                flags.clear();
                names.iter().all(|name| flags.set(name, true))
            }
            FlagList::Change(changes) => changes
                .iter()
                .all(|change| flags.set(change.name, change.set)),
        };
        if taken {
            Ok(())
        } else {
            Err(IniError::NotListed { line: self.number })
        }
    }

    /// The value of a field of a random variable, as `INI::parseGameClientRandomVariable` reads
    /// it: its low, its high, and its distribution, uniform when it names none; one the game
    /// draws as 0, `GAUSSIAN`, `TRIANGULAR`, `LOW_BIAS` or `HIGH_BIAS`, is 0
    /// (`GameClientRandomVariable::getValue`).
    pub(crate) fn random(&self) -> Result<Spread, IniError> {
        let spread = Spread {
            low: self.real_at(1)?,
            high: self.real_at(2)?,
        };
        let Some(distribution) = self.tokens.get(3) else {
            return Ok(spread);
        };
        let drawn = |name: &str| distribution.eq_ignore_ascii_case(name);
        if ["CONSTANT", "UNIFORM"].into_iter().any(drawn) {
            Ok(spread)
        } else if ["GAUSSIAN", "TRIANGULAR", "LOW_BIAS", "HIGH_BIAS"]
            .into_iter()
            .any(drawn)
        {
            Ok(Spread::ZERO)
        } else {
            Err(IniError::NotListed { line: self.number })
        }
    }

    /// A field's value, its second token.
    pub(crate) fn value(&self) -> Result<&str, IniError> {
        self.tokens
            .get(1)
            .copied()
            .ok_or(IniError::NoValue { line: self.number })
    }
}

/// The block whose first line is `head`, its lines the next of `lines`.
fn block<'a>(
    head: IniLine<'a>,
    lines: &mut impl Iterator<Item = IniLine<'a>>,
    kinds: BlockKinds<'_>,
) -> Result<IniBlock<'a>, IniError> {
    let word = head.word();
    if word == "end" {
        return Err(IniError::StrayEnd { line: head.number });
    }
    if let BlockKinds::One(kind) = kinds
        && word != kind
    {
        return Err(IniError::NotBlock { line: head.number });
    }
    let mut fields = Vec::new();
    for line in lines {
        if line.word() == "end" {
            return Ok(IniBlock { head, fields });
        }
        fields.push(line);
    }
    Err(IniError::Unclosed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_splits_at_equals_and_blanks_and_ends_at_a_comment() {
        let text = IniText::of(
            b"; a comment\r\nTerrain Sand\r\n  Texture=Sand.tga ; its file\r\n\tEnd\r\n\r\n=\n",
        );
        let lines = text.lines();
        let lines: Vec<(usize, &[&str], &str)> = lines
            .iter()
            .map(|line| (line.number, line.tokens, line.word()))
            .collect();
        assert_eq!(
            lines,
            [
                (2, &["Terrain", "Sand"][..], "terrain"),
                (3, &["Texture", "Sand.tga"], "texture"),
                (4, &["End"], "end"),
            ]
        );
        // A number's longest prefix, the rest ignored, as the game's scanners read it: Zero
        // Hour's own `GeometryHeight = 40v.0` is 40.
        let number = |token: &str| IniText::of(format!("Field = {token}\n").as_bytes());
        let real = |token: &str| number(token).lines().iter().next().unwrap().real().ok();
        let int = |token: &str| number(token).lines().iter().next().unwrap().int().ok();
        assert_eq!(real("40v.0"), Some(40.0));
        assert_eq!(real("-2.5e1x"), Some(-25.0));
        assert_eq!(real(".5."), Some(0.5));
        assert_eq!(real("3.e"), Some(3.0));
        assert_eq!(real("+1e+2"), Some(100.0));
        assert_eq!(real("inf"), Some(f32::INFINITY));
        assert_eq!(real("v40"), None);
        assert_eq!(real("-."), None);
        assert_eq!(int("12ab"), Some(12));
        assert_eq!(int("-7.5"), Some(-7));
        assert_eq!(int("x1"), None);
        // A coordinate's labels and values, cut at `:` too, the labels of either case.
        let coord = |text: &str| {
            let text = IniText::of(format!("Point = {text}\n").as_bytes());
            text.lines().iter().next().unwrap().coord(["X", "Y", "Z"])
        };
        assert_eq!(coord("X:-18.0  Y:  35.0   Z:0.0"), Ok([-18.0, 35.0, 0.0]));
        assert_eq!(coord("x: 60 y:35 z : 1.5"), Ok([60.0, 35.0, 1.5]));
        assert_eq!(coord("X:1 Z:2 Y:3"), Err(IniError::Coord { line: 1 }));
        assert_eq!(coord("X:1 Y:2"), Err(IniError::Coord { line: 1 }));
        // A byte past ASCII is one char of Latin-1.
        let latin = IniText::of(b"Name = \xe9t\xe9");
        assert_eq!(
            latin.lines().iter().next().unwrap().tokens,
            ["Name", "\u{e9}t\u{e9}"]
        );
    }

    #[test]
    fn a_files_blocks_end_at_their_end_and_hold_its_kinds_alone() {
        let text = IniText::of(b"Weapon Gun\n  Damage = 5\nEnd\nArmor Plate\nEnd\n");
        let lines = text.lines();
        let kinds = |kinds| {
            lines
                .blocks(kinds)
                .map(|block| block.map(|block| (block.head.word(), block.fields.len())))
                .collect::<Vec<_>>()
        };
        // Of any kind, both blocks, the weapon's one field; of the weapon's kind alone, the
        // armor's first line is an error.
        assert_eq!(
            kinds(BlockKinds::Any),
            [Ok(("weapon", 1)), Ok(("armor", 0))]
        );
        assert_eq!(
            kinds(BlockKinds::One("weapon")),
            [Ok(("weapon", 1)), Err(IniError::NotBlock { line: 4 })]
        );
        let name = |bytes: &[u8]| {
            let text = IniText::of(bytes);
            let lines = text.lines();
            let block = lines.blocks(BlockKinds::Any).next().unwrap().unwrap();
            block.name().map(str::to_owned)
        };
        assert_eq!(name(b"Weapon Gun\nEnd\n"), Ok("Gun".to_owned()));
        assert_eq!(name(b"GameData\nEnd\n"), Err(IniError::NoName { line: 1 }));
    }
}
