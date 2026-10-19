//! The XML documents a UPnP router serves, read within a bound: `xmltree` reads each element by
//! a call of its own, so a nesting with no bound would overflow the stack.

use xmltree::{Element, XMLNode};

/// The deepest nesting of a router's XML document the map reads, as `igd-next` bounded it.
const MAX_DEPTH: usize = 64;

/// The child elements of `parent` named `name`, in order.
pub(crate) fn children<'e>(
    parent: &'e Element,
    name: &'e str,
) -> impl Iterator<Item = &'e Element> + 'e {
    parent
        .children
        .iter()
        .filter_map(XMLNode::as_element)
        .filter(move |child| child.name == name)
}

/// The XML document `body` holds, when it reads and nests no deeper than `MAX_DEPTH`.
pub(crate) fn parse(body: &[u8]) -> Option<Element> {
    is_within_depth(body)
        .then(|| Element::parse(body).ok())
        .flatten()
}

/// Whether the elements of `body` nest no deeper than `MAX_DEPTH`, read before `xmltree`
/// recurses into them: a comment, a CDATA section, a declaration and a processing
/// instruction open none, and a tag ends at its first `>` out of its quotes.
fn is_within_depth(body: &[u8]) -> bool {
    let mut depth = 0_usize;
    let mut at = 0;
    while let Some(open) = body[at..].iter().position(|&byte| byte == b'<') {
        let open = at + open;
        let rest = &body[open..];
        let skip_to = |close: &[u8]| {
            rest.windows(close.len())
                .position(|window| window == close)
                .map(|end| open + end + close.len())
        };
        let next = if rest.starts_with(b"<!--") {
            skip_to(b"-->")
        } else if rest.starts_with(b"<![CDATA[") {
            skip_to(b"]]>")
        } else if rest.starts_with(b"<?") || rest.starts_with(b"<!") {
            skip_to(b">")
        } else {
            let mut quote = None;
            let end = rest.iter().position(|&byte| match quote {
                Some(open) => {
                    if byte == open {
                        quote = None;
                    }
                    false
                }
                None if byte == b'"' || byte == b'\'' => {
                    quote = Some(byte);
                    false
                }
                None => byte == b'>',
            });
            let Some(end) = end else {
                return true;
            };
            if rest.get(1) == Some(&b'/') {
                depth = depth.saturating_sub(1);
            } else if rest[end - 1] != b'/' {
                depth += 1;
                if depth > MAX_DEPTH {
                    return false;
                }
            }
            Some(open + end + 1)
        };
        let Some(next) = next else {
            return true;
        };
        at = next;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_nests_at_most_64_deep_however_its_tags_hide_a_bracket() {
        let nested = |depth: usize, open: &str| {
            let mut text = String::new();
            for _ in 0..depth {
                text.push_str(open);
            }
            for _ in 0..depth {
                text.push_str("</a>");
            }
            text
        };
        assert!(parse(nested(64, "<a>").as_bytes()).is_some());
        assert!(parse(nested(65, "<a>").as_bytes()).is_none());
        // A quoted `/>` closes no tag, and a comment or CDATA holds no tag.
        assert!(parse(nested(65, r#"<a x="/>">"#).as_bytes()).is_none());
        let hidden = format!(
            "<r><!-- <a> -->{}<![CDATA[</a></a>]]></r>",
            nested(63, "<a>")
        );
        assert!(parse(hidden.as_bytes()).is_some());
        let deep = format!("<r>{}<![CDATA[</a></a>]]></r>", nested(64, "<a>"));
        assert!(parse(deep.as_bytes()).is_none());
        assert!(parse(b"<a><b/><b/></a>").is_some());
    }
}
