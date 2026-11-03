// Admit complete ASCII service codes before they can enter diagnostics. Rejected
// values are omitted, rather than shortened or passed through redaction.
pub(crate) struct ServiceErrorCode<'a>(&'a str);

impl<'a> ServiceErrorCode<'a> {
    pub(crate) fn parse(value: &'a str) -> Option<Self> {
        ((1..=64).contains(&value.len())
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b".:_-".contains(&byte)))
        .then_some(Self(value))
    }

    pub(crate) fn as_str(&self) -> &'a str {
        self.0
    }
}
