//! Read-back predicates shared by schema generation and the AWS runtime.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadBackPredicate {
    AcmDnsDvo,
}

impl ReadBackPredicate {
    /// Schema enum input that controls applicability.
    pub const fn input_attribute(self) -> &'static str {
        match self {
            Self::AcmDnsDvo => "validation_method",
        }
    }
}
