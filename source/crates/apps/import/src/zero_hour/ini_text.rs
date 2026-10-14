use crate::zero_hour::error::IniError;

/// An INI file's text, as the game reads it: bytes as Latin-1 chars, each kept as one.
#[derive(Debug)]
pub(crate) struct IniText(String);

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

    /// The value of a field of a whole number.
    pub(crate) fn int(&self) -> Result<i32, IniError> {
        self.value()?.parse().map_err(|error| IniError::Number {
            line: self.number,
            error,
        })
    }

    /// The value of a field of a real number.
    pub(crate) fn real(&self) -> Result<f32, IniError> {
        self.value()?.parse().map_err(|error| IniError::Real {
            line: self.number,
            error,
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
        // A byte past ASCII is one char of Latin-1.
        let latin = IniText::of(b"Name = \xe9t\xe9");
        assert_eq!(
            latin.lines().next().unwrap().tokens,
            ["Name", "\u{e9}t\u{e9}"]
        );
    }
}
