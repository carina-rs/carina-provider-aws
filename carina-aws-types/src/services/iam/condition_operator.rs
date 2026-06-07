use carina_core::resource::{ConcreteValue, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ConditionOperator {
    pub qualifier: Option<ConditionQualifier>,
    pub base: ConditionOperatorBase,
    pub if_exists: bool,
}

/// Set-aware qualifier prefix on a [`ConditionOperator`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConditionQualifier {
    ForAllValues,
    ForAnyValue,
}

impl ConditionQualifier {
    /// Every defined qualifier, in the canonical AWS-doc order.
    pub const ALL: &'static [ConditionQualifier] = &[
        ConditionQualifier::ForAllValues,
        ConditionQualifier::ForAnyValue,
    ];

    /// Snake-case prefix written in the DSL (including the trailing `_`).
    pub const fn snake_prefix(self) -> &'static str {
        match self {
            ConditionQualifier::ForAllValues => "for_all_values_",
            ConditionQualifier::ForAnyValue => "for_any_value_",
        }
    }

    /// PascalCase prefix accepted by the AWS IAM API (including the trailing `:`).
    pub const fn aws_prefix(self) -> &'static str {
        match self {
            ConditionQualifier::ForAllValues => "ForAllValues:",
            ConditionQualifier::ForAnyValue => "ForAnyValue:",
        }
    }
}

/// Base IAM condition operator, without any qualifier or `_if_exists` suffix.
///
/// Marked `#[non_exhaustive]` so AWS adding a new operator does not
/// become a downstream SemVer break for code that pattern-matches on this
/// enum. The new variant goes into [`ConditionOperatorBase::ALL`] together
/// with the [`ConditionOperatorBase::snake`] / [`ConditionOperatorBase::aws`]
/// arms — the latter two are exhaustive matches so the compiler enforces
/// they stay complete, but `ALL` is hand-written and must be edited in
/// lockstep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConditionOperatorBase {
    StringEquals,
    StringNotEquals,
    StringEqualsIgnoreCase,
    StringNotEqualsIgnoreCase,
    StringLike,
    StringNotLike,
    NumericEquals,
    NumericNotEquals,
    NumericLessThan,
    NumericLessThanEquals,
    NumericGreaterThan,
    NumericGreaterThanEquals,
    DateEquals,
    DateNotEquals,
    DateLessThan,
    DateLessThanEquals,
    DateGreaterThan,
    DateGreaterThanEquals,
    Bool,
    BinaryEquals,
    IpAddress,
    NotIpAddress,
    ArnEquals,
    ArnNotEquals,
    ArnLike,
    ArnNotLike,
    Null,
}

impl ConditionOperatorBase {
    /// Every defined base operator, in the canonical AWS-doc order
    /// (String / Numeric / Date / Boolean / Binary / IP / ARN / Null).
    pub const ALL: &'static [ConditionOperatorBase] = &[
        ConditionOperatorBase::StringEquals,
        ConditionOperatorBase::StringNotEquals,
        ConditionOperatorBase::StringEqualsIgnoreCase,
        ConditionOperatorBase::StringNotEqualsIgnoreCase,
        ConditionOperatorBase::StringLike,
        ConditionOperatorBase::StringNotLike,
        ConditionOperatorBase::NumericEquals,
        ConditionOperatorBase::NumericNotEquals,
        ConditionOperatorBase::NumericLessThan,
        ConditionOperatorBase::NumericLessThanEquals,
        ConditionOperatorBase::NumericGreaterThan,
        ConditionOperatorBase::NumericGreaterThanEquals,
        ConditionOperatorBase::DateEquals,
        ConditionOperatorBase::DateNotEquals,
        ConditionOperatorBase::DateLessThan,
        ConditionOperatorBase::DateLessThanEquals,
        ConditionOperatorBase::DateGreaterThan,
        ConditionOperatorBase::DateGreaterThanEquals,
        ConditionOperatorBase::Bool,
        ConditionOperatorBase::BinaryEquals,
        ConditionOperatorBase::IpAddress,
        ConditionOperatorBase::NotIpAddress,
        ConditionOperatorBase::ArnEquals,
        ConditionOperatorBase::ArnNotEquals,
        ConditionOperatorBase::ArnLike,
        ConditionOperatorBase::ArnNotLike,
        ConditionOperatorBase::Null,
    ];

    /// Snake-case DSL spelling of this base operator.
    pub const fn snake(self) -> &'static str {
        match self {
            ConditionOperatorBase::StringEquals => "string_equals",
            ConditionOperatorBase::StringNotEquals => "string_not_equals",
            ConditionOperatorBase::StringEqualsIgnoreCase => "string_equals_ignore_case",
            ConditionOperatorBase::StringNotEqualsIgnoreCase => "string_not_equals_ignore_case",
            ConditionOperatorBase::StringLike => "string_like",
            ConditionOperatorBase::StringNotLike => "string_not_like",
            ConditionOperatorBase::NumericEquals => "numeric_equals",
            ConditionOperatorBase::NumericNotEquals => "numeric_not_equals",
            ConditionOperatorBase::NumericLessThan => "numeric_less_than",
            ConditionOperatorBase::NumericLessThanEquals => "numeric_less_than_equals",
            ConditionOperatorBase::NumericGreaterThan => "numeric_greater_than",
            ConditionOperatorBase::NumericGreaterThanEquals => "numeric_greater_than_equals",
            ConditionOperatorBase::DateEquals => "date_equals",
            ConditionOperatorBase::DateNotEquals => "date_not_equals",
            ConditionOperatorBase::DateLessThan => "date_less_than",
            ConditionOperatorBase::DateLessThanEquals => "date_less_than_equals",
            ConditionOperatorBase::DateGreaterThan => "date_greater_than",
            ConditionOperatorBase::DateGreaterThanEquals => "date_greater_than_equals",
            ConditionOperatorBase::Bool => "bool",
            ConditionOperatorBase::BinaryEquals => "binary_equals",
            ConditionOperatorBase::IpAddress => "ip_address",
            ConditionOperatorBase::NotIpAddress => "not_ip_address",
            ConditionOperatorBase::ArnEquals => "arn_equals",
            ConditionOperatorBase::ArnNotEquals => "arn_not_equals",
            ConditionOperatorBase::ArnLike => "arn_like",
            ConditionOperatorBase::ArnNotLike => "arn_not_like",
            ConditionOperatorBase::Null => "null",
        }
    }

    /// PascalCase AWS-API spelling of this base operator.
    pub const fn aws(self) -> &'static str {
        match self {
            ConditionOperatorBase::StringEquals => "StringEquals",
            ConditionOperatorBase::StringNotEquals => "StringNotEquals",
            ConditionOperatorBase::StringEqualsIgnoreCase => "StringEqualsIgnoreCase",
            ConditionOperatorBase::StringNotEqualsIgnoreCase => "StringNotEqualsIgnoreCase",
            ConditionOperatorBase::StringLike => "StringLike",
            ConditionOperatorBase::StringNotLike => "StringNotLike",
            ConditionOperatorBase::NumericEquals => "NumericEquals",
            ConditionOperatorBase::NumericNotEquals => "NumericNotEquals",
            ConditionOperatorBase::NumericLessThan => "NumericLessThan",
            ConditionOperatorBase::NumericLessThanEquals => "NumericLessThanEquals",
            ConditionOperatorBase::NumericGreaterThan => "NumericGreaterThan",
            ConditionOperatorBase::NumericGreaterThanEquals => "NumericGreaterThanEquals",
            ConditionOperatorBase::DateEquals => "DateEquals",
            ConditionOperatorBase::DateNotEquals => "DateNotEquals",
            ConditionOperatorBase::DateLessThan => "DateLessThan",
            ConditionOperatorBase::DateLessThanEquals => "DateLessThanEquals",
            ConditionOperatorBase::DateGreaterThan => "DateGreaterThan",
            ConditionOperatorBase::DateGreaterThanEquals => "DateGreaterThanEquals",
            ConditionOperatorBase::Bool => "Bool",
            ConditionOperatorBase::BinaryEquals => "BinaryEquals",
            ConditionOperatorBase::IpAddress => "IpAddress",
            ConditionOperatorBase::NotIpAddress => "NotIpAddress",
            ConditionOperatorBase::ArnEquals => "ArnEquals",
            ConditionOperatorBase::ArnNotEquals => "ArnNotEquals",
            ConditionOperatorBase::ArnLike => "ArnLike",
            ConditionOperatorBase::ArnNotLike => "ArnNotLike",
            ConditionOperatorBase::Null => "Null",
        }
    }
}

impl ConditionOperator {
    /// Construct a [`ConditionOperator`] from its three structural pieces.
    /// Always succeeds — the type does not police IAM semantics; AWS does.
    pub const fn new(
        qualifier: Option<ConditionQualifier>,
        base: ConditionOperatorBase,
        if_exists: bool,
    ) -> Self {
        Self {
            qualifier,
            base,
            if_exists,
        }
    }

    /// Snake-case DSL spelling (e.g. `for_all_values_string_equals_if_exists`).
    pub fn to_snake(self) -> String {
        let prefix = self.qualifier.map_or("", ConditionQualifier::snake_prefix);
        let suffix = if self.if_exists { "_if_exists" } else { "" };
        format!("{prefix}{}{suffix}", self.base.snake())
    }

    /// PascalCase AWS-API spelling (e.g. `ForAllValues:StringEqualsIfExists`).
    pub fn to_aws(self) -> String {
        let prefix = self.qualifier.map_or("", ConditionQualifier::aws_prefix);
        let suffix = if self.if_exists { "IfExists" } else { "" };
        format!("{prefix}{}{suffix}", self.base.aws())
    }

    /// Parse a snake-case DSL spelling. Returns `None` if no variant matches.
    pub fn from_snake(snake: &str) -> Option<ConditionOperator> {
        let (rest, if_exists) = match snake.strip_suffix("_if_exists") {
            Some(base) => (base, true),
            None => (snake, false),
        };
        for &q in ConditionQualifier::ALL {
            if let Some(base) = rest.strip_prefix(q.snake_prefix()) {
                return ConditionOperatorBase::ALL
                    .iter()
                    .copied()
                    .find(|b| b.snake() == base)
                    .map(|base| ConditionOperator {
                        qualifier: Some(q),
                        base,
                        if_exists,
                    });
            }
        }
        ConditionOperatorBase::ALL
            .iter()
            .copied()
            .find(|b| b.snake() == rest)
            .map(|base| ConditionOperator {
                qualifier: None,
                base,
                if_exists,
            })
    }

    /// Parse a PascalCase AWS-API spelling. Returns `None` if no variant matches.
    pub fn from_aws(pascal: &str) -> Option<ConditionOperator> {
        let (rest, if_exists) = match pascal.strip_suffix("IfExists") {
            Some(base) => (base, true),
            None => (pascal, false),
        };
        for &q in ConditionQualifier::ALL {
            if let Some(base) = rest.strip_prefix(q.aws_prefix()) {
                return ConditionOperatorBase::ALL
                    .iter()
                    .copied()
                    .find(|b| b.aws() == base)
                    .map(|base| ConditionOperator {
                        qualifier: Some(q),
                        base,
                        if_exists,
                    });
            }
        }
        ConditionOperatorBase::ALL
            .iter()
            .copied()
            .find(|b| b.aws() == rest)
            .map(|base| ConditionOperator {
                qualifier: None,
                base,
                if_exists,
            })
    }

    /// Every `(qualifier, base, if_exists)` cross-product, in a stable order
    /// (outer = base in AWS-doc order, inner = base → `_if_exists` →
    /// qualified bases → qualified `_if_exists`). The schema's
    /// `ConditionOperator` `StringEnum` values and the validator's
    /// "valid operators" suggestion are both derived from this single list,
    /// so they cannot drift from each other or from `from_snake` / `from_aws`.
    ///
    /// The cross-product is unconditional, so semantically-nonsense
    /// combinations like `for_all_values_null` or `bool_if_exists` are also
    /// emitted. Intentional: AWS is the authority on what IAM evaluates at
    /// apply time, so carina does not pre-judge IAM semantics here.
    pub fn all() -> impl Iterator<Item = ConditionOperator> {
        ConditionOperatorBase::ALL.iter().copied().flat_map(|base| {
            let mut spellings: Vec<ConditionOperator> =
                Vec::with_capacity(2 + 2 * ConditionQualifier::ALL.len());
            spellings.push(ConditionOperator {
                qualifier: None,
                base,
                if_exists: false,
            });
            spellings.push(ConditionOperator {
                qualifier: None,
                base,
                if_exists: true,
            });
            for &q in ConditionQualifier::ALL {
                spellings.push(ConditionOperator {
                    qualifier: Some(q),
                    base,
                    if_exists: false,
                });
                spellings.push(ConditionOperator {
                    qualifier: Some(q),
                    base,
                    if_exists: true,
                });
            }
            spellings
        })
    }
}

impl std::fmt::Display for ConditionOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_aws())
    }
}

/// Every snake_case condition-operator spelling the type system admits.
///
/// Delegates to [`ConditionOperator::all`] so the schema's `StringEnum`
/// values and the validator's suggestion list both flow from the same
/// type-level source; the schema cannot drift from `ConditionOperator::from_snake`.
pub(super) fn all_condition_operator_snake_forms() -> Vec<String> {
    ConditionOperator::all().map(|op| op.to_snake()).collect()
}

/// Convert a snake_case condition operator to its PascalCase AWS form.
/// Returns `None` if the operator is unknown.
///
/// Thin string-boundary wrapper over [`ConditionOperator::from_snake`] +
/// [`ConditionOperator::to_aws`] so callers that thread `&str` through
/// `unwrap_or_else(|| k.clone())` keep working. Prefer the typed API
/// inside this crate.
pub fn condition_operator_to_aws(snake: &str) -> Option<String> {
    ConditionOperator::from_snake(snake).map(|op| op.to_aws())
}

/// Convert a PascalCase AWS condition operator to snake_case DSL form.
/// Returns `None` if the operator is unknown.
///
/// Thin string-boundary wrapper over [`ConditionOperator::from_aws`] +
/// [`ConditionOperator::to_snake`].
pub fn condition_operator_to_snake(pascal: &str) -> Option<String> {
    ConditionOperator::from_aws(pascal).map(|op| op.to_snake())
}

/// Check if a string is a valid snake_case condition operator.
pub fn is_valid_condition_operator(key: &str) -> bool {
    ConditionOperator::from_snake(key).is_some()
}

/// Validate condition operators in a parsed IAM policy document.
///
/// Walks the document looking for `condition` maps and validates that
/// all operator keys are valid snake_case condition operators.
pub fn validate_condition_operators(value: &Value) -> Result<(), String> {
    let Value::Concrete(ConcreteValue::Map(doc)) = value else {
        return Ok(());
    };
    // Look for "statement" list
    let Some(Value::Concrete(ConcreteValue::List(statements))) = doc.get("statement") else {
        return Ok(());
    };
    for (i, stmt) in statements.iter().enumerate() {
        let Value::Concrete(ConcreteValue::Map(stmt_map)) = stmt else {
            continue;
        };
        let Some(Value::Concrete(ConcreteValue::Map(condition))) = stmt_map.get("condition") else {
            continue;
        };
        for key in condition.keys() {
            if !is_valid_condition_operator(key) {
                let valid_operators: Vec<&'static str> = ConditionOperatorBase::ALL
                    .iter()
                    .map(|b| b.snake())
                    .collect();
                return Err(format!(
                    "statement[{}]: unknown condition operator '{}'. \
                     Valid operators: {} \
                     (prefix with for_all_values_ or for_any_value_ for set operators, \
                     append _if_exists for conditional variants)",
                    i,
                    key,
                    valid_operators.join(", ")
                ));
            }
        }
    }
    Ok(())
}
