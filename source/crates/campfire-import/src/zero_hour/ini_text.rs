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
