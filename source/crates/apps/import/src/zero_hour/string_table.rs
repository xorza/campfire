use std::collections::BTreeMap;

use crate::zero_hour::byte_cursor::ByteCursor;
use crate::zero_hour::error::CsfError;

/// The game's string table, `generals.csf`, as `GameTextManager::parseCSF` reads it: each label's
/// first string, its spaces made as `stripSpaces` makes them, by its label in lowercase, as the
/// game looks a label up of either case.
#[derive(Debug, Default)]
pub(crate) struct StringTable {
    texts: BTreeMap<String, String>,
}

/// The tags of a CSF file, as the game reads each, a little-endian `i32` of four letters.
const CSF: [u8; 4] = *b" FSC";
const LABEL: [u8; 4] = *b" LBL";
const STRING: [u8; 4] = *b" RTS";
const STRING_WITH_WAVE: [u8; 4] = *b"WRTS";

impl StringTable {
    /// The table of `bytes`: a header of its tag, its version, its counts of labels and strings,
    /// the bytes to pass over and its language; then, until the bytes end, each label, the tag
    /// `LBL `, its count of strings, its name; each string the tag `STR ` or `STRW`, its length
    /// in UTF-16 units and those units each inverted bit by bit, and for `STRW` a speech file's
    /// name. A label read again keeps its first text, as the game's search finds the first.
    pub(crate) fn read(bytes: &[u8]) -> Result<Self, CsfError> {
        let mut at = ByteCursor::new(bytes);
        if at.take(4)? != CSF {
            return Err(CsfError::NotCsf);
        }
        let [_version, _labels, _strings, skip, _language] = [(); 5]
            .map(|()| at.i32())
            .map(|field| field.map_err(CsfError::from));
        let skip = usize::try_from(skip?).map_err(|_| CsfError::NotCsf)?;
        at.take(skip)?;
        let mut table = Self::default();
        let mut index = 0;
        while !at.is_empty() {
            if at.take(4)? != LABEL {
                return Err(CsfError::NotLabel { index });
            }
            let strings = at.i32()?;
            let name = Self::bytes(&mut at)?;
            let label: String = name.iter().map(|&byte| char::from(byte)).collect();
            let mut first = None;
            for _ in 0..strings {
                let tag = at.take(4)?;
                if tag != STRING && tag != STRING_WITH_WAVE {
                    return Err(CsfError::NotString { index });
                }
                let len = usize::try_from(at.i32()?).map_err(|_| CsfError::NotString { index })?;
                let units: Vec<u16> = (0..len)
                    .map(|_| at.u16().map(|unit| !unit))
                    .collect::<Result<_, _>>()?;
                if tag == STRING_WITH_WAVE {
                    Self::bytes(&mut at)?;
                }
                if first.is_none() {
                    let text =
                        String::from_utf16(&units).map_err(|_| CsfError::NotText { index })?;
                    first = Some(Self::stripped(&text));
                }
            }
            table
                .texts
                .entry(label.to_ascii_lowercase())
                .or_insert_with(|| first.unwrap_or_default());
            index += 1;
        }
        Ok(table)
    }

    /// The text of `label`, of either case; none for a label the table lacks.
    pub(crate) fn get(&self, label: &str) -> Option<&str> {
        self.texts
            .get(&label.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// A length, an `i32`, and that many bytes.
    fn bytes<'a>(at: &mut ByteCursor<'a>) -> Result<&'a [u8], CsfError> {
        let len = usize::try_from(at.i32()?).map_err(|_| CsfError::NotCsf)?;
        Ok(at.take(len)?)
    }

    /// `text` as `stripSpaces` leaves it, up to its first NUL: a run of spaces as one, none at
    /// its start, after a newline or a tab, before one, or at its end.
    fn stripped(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut skip_all = true;
        for char in text.chars().take_while(|&char| char != '\0') {
            match char {
                ' ' if skip_all || out.ends_with(' ') => {}
                '\n' | '\t' => {
                    if out.ends_with(' ') {
                        out.pop();
                    }
                    out.push(char);
                    skip_all = true;
                }
                _ => {
                    out.push(char);
                    skip_all = false;
                }
            }
        }
        if out.ends_with(' ') {
            out.pop();
        }
        out
    }
}

#[cfg(test)]
pub(crate) mod internals {
    /// A label of a fixture CSF file, and its strings, each with the speech file of a `STRW`.
    pub(crate) type Label<'a> = (&'a str, &'a [(&'a str, Option<&'a str>)]);

    /// `Data\English\generals.csf` of the fixture install: the names of the dozer's and the
    /// barracks' buttons, each with its hotkey's marker, and the barracks' with braces, which a
    /// Fluent value escapes; the ranger's button names a label it lacks.
    pub(crate) fn strings() -> Vec<u8> {
        csf(&[
            ("CONTROLBAR:ConstructAmericaDozer", &[("&Dozer", None)]),
            (
                "CONTROLBAR:ConstructAmericaBarracks",
                &[("Ba&rracks && {Yard}", Some("barracks"))],
            ),
        ])
    }

    /// The bytes of a CSF file of `labels`.
    pub(crate) fn csf(labels: &[Label<'_>]) -> Vec<u8> {
        let int = |value: usize| i32::try_from(value).unwrap().to_le_bytes();
        let mut bytes = b" FSC".to_vec();
        for field in [3, labels.len(), 0, 0, 0] {
            bytes.extend(int(field));
        }
        for (label, strings) in labels {
            bytes.extend(b" LBL");
            bytes.extend(int(strings.len()));
            bytes.extend(int(label.len()));
            bytes.extend(label.as_bytes());
            for (text, wave) in *strings {
                let units: Vec<u16> = text.encode_utf16().collect();
                bytes.extend(if wave.is_some() { b"WRTS" } else { b" RTS" });
                bytes.extend(int(units.len()));
                bytes.extend(units.iter().flat_map(|unit| (!unit).to_le_bytes()));
                if let Some(wave) = wave {
                    bytes.extend(int(wave.len()));
                    bytes.extend(wave.as_bytes());
                }
            }
        }
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::string_table::internals::csf;

    #[test]
    fn a_label_takes_its_first_string_its_spaces_stripped_by_either_case() {
        let bytes = csf(&[
            ("CONTROLBAR:ConstructAmericaDozer", &[("&Dozer", None)]),
            (
                "CONTROLBAR:Two",
                &[("  first  of   two ", Some("speech")), ("second", None)],
            ),
            ("Lines", &[(" a \n  b\t c ", None)]),
            ("Empty", &[]),
            ("controlbar:constructamericadozer", &[("later", None)]),
        ]);
        let table = StringTable::read(&bytes).unwrap();
        assert_eq!(
            table.get("CONTROLBAR:ConstructAmericaDozer"),
            Some("&Dozer")
        );
        assert_eq!(
            table.get("controlbar:CONSTRUCTAMERICADOZER"),
            Some("&Dozer")
        );
        assert_eq!(table.get("CONTROLBAR:Two"), Some("first of two"));
        assert_eq!(table.get("Lines"), Some("a\nb\tc"));
        assert_eq!(table.get("Empty"), Some(""));
        assert_eq!(table.get("Gone"), None);
        // A wrong tag, a label's or a string's, and bytes cut short are refused.
        assert_eq!(StringTable::read(b"CSF ").unwrap_err(), CsfError::NotCsf);
        let mut wrong = bytes.clone();
        wrong[24..28].copy_from_slice(b"LBL ");
        assert_eq!(
            StringTable::read(&wrong).unwrap_err(),
            CsfError::NotLabel { index: 0 }
        );
        let cut = &bytes[..bytes.len() - 1];
        assert_eq!(StringTable::read(cut).unwrap_err(), CsfError::Short);
    }
}
