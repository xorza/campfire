use crate::zero_hour::error::IniError;

/// An INI file's text, as the game reads it: bytes as Latin-1 chars, each kept as one.
#[derive(Debug)]
pub(crate) struct IniText(String);

/// A field's list of flags, as `INI::parseBitString32` and `KindOf` read it: `NONE`, which clears
/// every flag and ends the list; names, which clear every flag and set those; or names of `+` and
/// `-`, which set and clear those and keep the others.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FlagList<'a> {
    Clear,
    Only(Vec<&'a str>),
    Change(Vec<FlagChange<'a>>),
}

/// A flag a list of `+` and `-` sets, or clears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FlagChange<'a> {
    pub(crate) name: &'a str,
    pub(crate) set: bool,
}

/// A line of an INI file that holds a token: its number, from 1, and its tokens, as the game's
/// reader splits them: the text before a `;`, cut at every `=` and every char up to a space.
#[derive(Debug)]
pub(crate) struct IniLine<'a> {
    pub(crate) number: usize,
    pub(crate) tokens: Vec<&'a str>,
}

impl IniText {
    pub(crate) fn of(bytes: &[u8]) -> IniText {
        IniText(bytes.iter().map(|&byte| char::from(byte)).collect())
    }

    /// Its lines that hold a token, in order.
    pub(crate) fn lines(&self) -> impl Iterator<Item = IniLine<'_>> {
        self.0.split('\n').enumerate().filter_map(|(at, raw)| {
            let content = raw.split(';').next().unwrap_or_default();
            let tokens: Vec<&str> = content
                .split(|c: char| c <= ' ' || c == '=')
                .filter(|token| !token.is_empty())
                .collect();
            (!tokens.is_empty()).then_some(IniLine {
                number: at + 1,
                tokens,
            })
        })
    }
}

impl IniLine<'_> {
    /// Its first token, folded to lowercase, as the game matches a keyword.
    pub(crate) fn word(&self) -> String {
        self.tokens[0].to_ascii_lowercase()
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
        let value = self.value()?;
        let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
        let end = value.len() - digits.len() + IniLine::digits(digits);
        value[..end].parse().map_err(|error| IniError::Number {
            line: self.number,
            error,
        })
    }

    /// The value of a field of a real number: its longest prefix of one, as `sscanf`'s `%f` and
    /// the game's `from_chars` read it, the rest of the token ignored, so a token of `40v.0` is
    /// 40; or the whole token, for one of infinity or of no number.
    pub(crate) fn real(&self) -> Result<f32, IniError> {
        let value = self.value()?;
        let real = value
            .parse()
            .or_else(|_| value[..IniLine::real_prefix(value)].parse());
        real.map_err(|error| IniError::Real {
            line: self.number,
            error,
        })
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

    /// A field's value, its second token.
    pub(crate) fn value(&self) -> Result<&str, IniError> {
        self.tokens
            .get(1)
            .copied()
            .ok_or(IniError::NoValue { line: self.number })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_splits_at_equals_and_blanks_and_ends_at_a_comment() {
        let text = IniText::of(
            b"; a comment\r\nTerrain Sand\r\n  Texture=Sand.tga ; its file\r\n\tEnd\r\n\r\n=\n",
        );
        let lines: Vec<(usize, Vec<&str>)> = text
            .lines()
            .map(|line| (line.number, line.tokens))
            .collect();
        assert_eq!(
            lines,
            [
                (2, vec!["Terrain", "Sand"]),
                (3, vec!["Texture", "Sand.tga"]),
                (4, vec!["End"]),
            ]
        );
        // A number's longest prefix, the rest ignored, as the game's scanners read it: Zero
        // Hour's own `GeometryHeight = 40v.0` is 40.
        let number = |token: &str| IniText::of(format!("Field = {token}\n").as_bytes());
        let real = |token: &str| number(token).lines().next().unwrap().real().ok();
        let int = |token: &str| number(token).lines().next().unwrap().int().ok();
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
        // A byte past ASCII is one char of Latin-1.
        let latin = IniText::of(b"Name = \xe9t\xe9");
        assert_eq!(
            latin.lines().next().unwrap().tokens,
            ["Name", "\u{e9}t\u{e9}"]
        );
    }
}
