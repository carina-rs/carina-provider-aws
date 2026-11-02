use std::borrow::Cow;

/// Description text whose origin determines how each output format escapes it.
/// Keep both fields private so consumers must choose a format before emitting.
#[derive(Debug, Clone)]
pub struct Description {
    text: String,
    origin: Origin,
}

#[derive(Debug, Clone, Copy)]
enum Origin {
    SmithyPlainText,
    AuthoredMarkdown,
}

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
        Self::with_origin(
            &html_escape::decode_html_entities(&text),
            Origin::SmithyPlainText,
        )
    }

    /// Normalize authored text without interpreting tags or HTML entities.
    pub fn authored(text: &str) -> Self {
        Self::with_origin(text, Origin::AuthoredMarkdown)
    }

    fn with_origin(text: &str, origin: Origin) -> Self {
        Self {
            text: text.split_whitespace().collect::<Vec<_>>().join(" "),
            origin,
        }
    }

    /// Emit the contents of a Rust string literal from the cleaned text.
    /// The optional byte limit applies before escaping; the ellipsis is extra.
    pub fn rust_string_literal(&self, max_bytes: Option<usize>) -> String {
        let text = &self.text;
        let end = text.floor_char_boundary(max_bytes.unwrap_or(text.len()).min(text.len()));
        let mut truncated = text[..end].to_string();
        if end < text.len() {
            truncated.push_str("...");
        }
        truncated.replace('\\', "\\\\").replace('"', "\\\"")
    }

    /// Emit authored Markdown unchanged, or escape Smithy plain text.
    /// HTML characters become entities; all other ASCII punctuation is escaped
    /// to prevent Markdown formatting, autolinks, and table delimiters.
    pub fn markdown(&self) -> Cow<'_, str> {
        match self.origin {
            Origin::AuthoredMarkdown => Cow::Borrowed(&self.text),
            Origin::SmithyPlainText => {
                let mut escaped = String::with_capacity(self.text.len());
                for character in self.text.chars() {
                    match character {
                        '<' => escaped.push_str("&lt;"),
                        '>' => escaped.push_str("&gt;"),
                        '&' => escaped.push_str("&amp;"),
                        punctuation if punctuation.is_ascii_punctuation() => {
                            escaped.push('\\');
                            escaped.push(punctuation);
                        }
                        _ => escaped.push(character),
                    }
                }
                Cow::Owned(escaped)
            }
        }
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
            text.markdown(),
            "Use `<bucket>.s3.amazonaws.com`, <p>literal</p>, &lt; and &amp;. Next."
        );
    }

    #[test]
    fn smithy_decodes_named_decimal_and_hex_entities_once() {
        let text = Description::from_smithy_html(
            "<p>&lt;code&gt;literal&lt;/code&gt; &amp; &quot;name&quot; &#39;x&#39; &#x1F980; &nbsp; &amp;lt;</p>",
        );
        assert_eq!(
            text.rust_string_literal(None),
            r#"<code>literal</code> & \"name\" 'x' 🦀 &lt;"#
        );
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
            assert_eq!(text.rust_string_literal(None), "content", "{tag}");
        }
    }

    #[test]
    fn smithy_handles_attributes_self_closing_tags_and_inline_spacing() {
        let text = Description::from_smithy_html(
            "<p><a href=\"https://example.com/?q=>\">a</a><code/>.<code>word</code>s</p><p>Next.</p>",
        );
        assert_eq!(text.rust_string_literal(None), "a.words Next.");
    }

    #[test]
    fn smithy_preserves_unknown_tags_and_incomplete_angle_brackets() {
        let text = Description::from_smithy_html(
            "<p><account-id> <Code> <code_name> <Grantee type=\"Group\">x</Grantee> <key> < 5</p> tail <p",
        );
        assert_eq!(
            text.rust_string_literal(None),
            r#"<account-id> <Code> <code_name> <Grantee type=\"Group\">x</Grantee> <key> < 5 tail <p"#
        );
    }

    #[test]
    fn smithy_markdown_renders_punctuation_as_literal_text() {
        let plain = r"1. <bucket> & **bold** _em_ `code` [link](#target) ![image](url) ~~strike~~ | \path https://example.com a@example.com";
        let description = Description::from_smithy_html(&format!("<p>{plain}</p>"));
        let mut options = comrak::Options::default();
        options.extension.table = true;
        options.extension.strikethrough = true;
        options.extension.autolink = true;
        let html = comrak::markdown_to_html(&description.markdown(), &options);
        assert_eq!(
            html,
            format!("<p>{}</p>\n", html_escape::encode_text(plain))
        );
    }

    #[test]
    fn authored_markdown_keeps_formatting_and_backticked_placeholders() {
        let description = Description::authored("**bold** `<bucket>.s3.amazonaws.com` &amp;");
        let markdown = description.markdown();
        assert_eq!(markdown, "**bold** `<bucket>.s3.amazonaws.com` &amp;");
        let html = comrak::markdown_to_html(&markdown, &comrak::Options::default());
        assert_eq!(
            html,
            "<p><strong>bold</strong> <code>&lt;bucket&gt;.s3.amazonaws.com</code> &amp;</p>\n"
        );
    }
}
