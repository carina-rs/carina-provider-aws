/// Plain text whose source has explicitly selected how markup is handled.
///
/// Keep the text private so consumers cannot accidentally emit raw Smithy HTML
/// or strip placeholders from authored descriptions.
#[derive(Debug, Clone)]
pub struct Description(String);

impl Description {
    /// Convert Smithy documentation HTML to normalized plain text.
    pub fn from_smithy_html(html: &str) -> Self {
        let mut text = String::with_capacity(html.len());
        let mut remaining = html;
        while let Some(open) = remaining.find('<') {
            text.push_str(&remaining[..open]);
            remaining = &remaining[open + 1..];
            if let Some((end, block)) = html_tag_end(remaining) {
                if block {
                    text.push(' ');
                }
                remaining = &remaining[end + 1..];
            } else {
                text.push('<');
            }
        }
        text.push_str(remaining);

        // Decode only after stripping: &lt;code&gt; describes literal text,
        // not markup that should be removed in a second pass.
        Self::authored(&html_escape::decode_html_entities(&text))
    }

    /// Normalize authored text without interpreting tags or HTML entities.
    pub fn authored(text: &str) -> Self {
        Self(text.split_whitespace().collect::<Vec<_>>().join(" "))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Return the closing `>` byte offset and whether a tag separates text blocks.
fn html_tag_end(after_open: &str) -> Option<(usize, bool)> {
    let tag = after_open.strip_prefix('/').unwrap_or(after_open);
    let name_end = tag
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(tag.len());
    let name = &tag[..name_end];
    // Measured all 19,201 documentation traits in the 11 cached models used by
    // scripts/generate-schemas-smithy.sh: these are the 14 markup names present.
    // Match their spelling exactly: e.g. <Code> is an S3 XML example, not <code>.
    let block = match name {
        "a" | "b" | "code" | "i" => false,
        "dd" | "dl" | "dt" | "fullname" | "important" | "li" | "note" | "ol" | "p" | "ul" => true,
        _ => return None,
    };
    if !tag[name_end..].starts_with(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>') {
        return None;
    }

    // Attribute values can contain `>`; only an unquoted `>` closes the tag.
    let mut quote = None;
    for (index, character) in after_open.char_indices() {
        match (quote, character) {
            (Some(open), close) if open == close => quote = None,
            (None, '\'' | '"') => quote = Some(character),
            (None, '>') => return Some((index, block)),
            (None, '<') => return None,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_text_only_normalizes_whitespace() {
        let text = Description::authored(
            " \tUse `<bucket>.s3.amazonaws.com`, <p>literal</p>, &lt; and &amp;.\r\n Next.\u{a0}",
        );
        assert_eq!(
            text.as_str(),
            "Use `<bucket>.s3.amazonaws.com`, <p>literal</p>, &lt; and &amp;. Next."
        );
    }

    #[test]
    fn smithy_decodes_named_decimal_and_hex_entities_once() {
        let text = Description::from_smithy_html(
            "<p>&lt;code&gt;literal&lt;/code&gt; &amp; &quot;name&quot; &#39;x&#39; &#x1F980; &nbsp; &amp;lt;</p>",
        );
        assert_eq!(text.as_str(), "<code>literal</code> & \"name\" 'x' 🦀 &lt;");
    }

    #[test]
    fn smithy_strips_every_measured_markup_name() {
        for tag in [
            "a",
            "b",
            "code",
            "dd",
            "dl",
            "dt",
            "fullname",
            "i",
            "important",
            "li",
            "note",
            "ol",
            "p",
            "ul",
        ] {
            let text = Description::from_smithy_html(&format!("<{tag}>content</{tag}>"));
            assert_eq!(text.as_str(), "content", "{tag}");
        }
    }

    #[test]
    fn smithy_handles_attributes_self_closing_tags_and_inline_spacing() {
        let text = Description::from_smithy_html(
            "<p><a href=\"https://example.com/?q=>\">a</a><code/>.<code>word</code>s</p><p>Next.</p>",
        );
        assert_eq!(text.as_str(), "a.words Next.");
    }

    #[test]
    fn smithy_preserves_unknown_tags_and_incomplete_angle_brackets() {
        let text = Description::from_smithy_html(
            "<p><account-id> <Code> <code_name> <Grantee type=\"Group\">x</Grantee> <key> < 5</p> tail <p",
        );
        assert_eq!(
            text.as_str(),
            "<account-id> <Code> <code_name> <Grantee type=\"Group\">x</Grantee> <key> < 5 tail <p"
        );
    }
}
