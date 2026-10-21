use crate::workspace::sources;

/// The most lines an inline test module holds before it moves to a `tests.rs` of its own.
const MOST_LINES: usize = 150;

/// The most of its file's lines, in percent, an inline test module holds.
const MOST_PERCENT: usize = 40;

/// An inline test module past its bound: its lines, and its file's.
#[derive(Debug, PartialEq, Eq)]
struct FatModule {
    lines: usize,
    file: usize,
}

/// Each inline test module of `text`, a `mod tests { … }` that `#[cfg(test)]` gates, at any
/// depth, that holds more than `MOST_LINES` lines or more than `MOST_PERCENT` of the file's: its
/// lines from its first attribute to its closing brace, which rustfmt indents as its declaration.
fn fat_test_modules(text: &str) -> Vec<FatModule> {
    let lines: Vec<&str> = text.lines().collect();
    let mut fat = Vec::new();
    for (declared, line) in lines.iter().enumerate() {
        let code = line.trim_start();
        let indent = &line[..line.len() - code.len()];
        if !code
            .strip_prefix("pub(crate) ")
            .unwrap_or(code)
            .starts_with("mod tests {")
        {
            continue;
        }
        let attributes = lines[..declared]
            .iter()
            .rev()
            .take_while(|line| {
                let attribute = line.strip_prefix(indent).unwrap_or_default();
                attribute.starts_with("#[") || attribute.starts_with("///")
            })
            .count();
        let first = declared - attributes;
        let gate = format!("{indent}#[cfg(test)]");
        if !lines[first..declared].contains(&gate.as_str()) {
            continue;
        }
        let end = format!("{indent}}}");
        let Some(close) = lines[declared..].iter().position(|&line| line == end) else {
            continue;
        };
        let module = FatModule {
            lines: close + attributes + 1,
            file: lines.len(),
        };
        if module.lines > MOST_LINES || module.lines * 100 > module.file * MOST_PERCENT {
            fat.push(module);
        }
    }
    fat
}

#[test]
fn an_inline_test_module_holds_at_most_150_lines_and_40_percent_of_its_file() {
    let fat: Vec<String> = sources()
        .iter()
        .flat_map(|file| {
            let modules = fat_test_modules(&file.text).into_iter();
            modules
                .map(|module| format!("{}: {} of {} lines", file.path, module.lines, module.file))
        })
        .collect();
    assert_eq!(fat, Vec::<String>::new());
}

#[test]
fn the_check_measures_each_gated_inline_test_module_against_its_file() {
    let file = |code: usize, tests: usize| {
        let mut text = "fn f() {}\n".repeat(code);
        text.push_str("\n#[cfg(test)]\nmod tests {\n");
        text.push_str(&"    fn g() {}\n".repeat(tests));
        text.push_str("}\n");
        text
    };
    let fat = |lines: usize, file: usize| vec![FatModule { lines, file }];
    // A module of 2 + 4 + 1 = 7 lines of 10 + 1 + 7 = 18 is 38.9 %; of 8 + 1 + 7 = 16, 43.8 %.
    assert_eq!(fat_test_modules(&file(10, 4)), []);
    assert_eq!(fat_test_modules(&file(8, 4)), fat(7, 16));
    // Of 151 lines it is past its bound however long its file: 2 + 148 + 1.
    assert_eq!(fat_test_modules(&file(1000, 148)), fat(151, 1152));
    assert_eq!(fat_test_modules(&file(1000, 147)), []);
    // A module of its own file, an ungated one and another name are none of the check's.
    assert_eq!(fat_test_modules("#[cfg(test)]\nmod tests;\n"), []);
    let ungated = file(1, 10).replace("#[cfg(test)]\n", "");
    assert_eq!(fat_test_modules(&ungated), []);
    let samples = file(1, 10).replace("mod tests {", "mod samples {");
    assert_eq!(fat_test_modules(&samples), []);
    // An attribute or a doc before the gate counts with the module.
    let documented = file(8, 4).replace("#[cfg(test)]\n", "/// Its tests.\n#[cfg(test)]\n");
    assert_eq!(fat_test_modules(&documented), fat(8, 17));
    // A module inside another ends at the brace of its own indent: 2 + 10 + 1 = 13 lines of
    // 1 + 1 + 13 + 1 = 16; and each module of a file counts, here two of 7 lines of 1 + 7 + 7 + 1
    // = 16 each, 43.8 %.
    let nested = format!(
        "fn f() {{}}\nmod bench {{\n    #[cfg(test)]\n    mod tests {{\n{}    }}\n}}\n",
        "        fn g() {}\n".repeat(10)
    );
    assert_eq!(fat_test_modules(&nested), fat(13, 16));
    let module = format!(
        "#[cfg(test)]\nmod tests {{\n{}}}\n",
        "    fn g() {}\n".repeat(4)
    );
    let twice = format!(
        "fn f() {{}}\n{module}{}",
        module.replace("mod tests", "pub(crate) mod tests")
    );
    let twice = format!("{twice}fn k() {{}}\n");
    assert_eq!(
        fat_test_modules(&twice),
        [
            FatModule { lines: 7, file: 16 },
            FatModule { lines: 7, file: 16 }
        ]
    );
}
