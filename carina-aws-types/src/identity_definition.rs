//! Reusable catalog definitions never contain declaration-local member layers.
use carina_core::schema::{
    AttributeType, CustomValidator, DslTransform, PatternSpec, TypeIdentity,
};

/// A catalog constructor defines an identity, never an attribute slot.
/// There is deliberately no conversion from an arbitrary AttributeType.
#[derive(Clone)]
pub struct IdentityDefinition(AttributeType);

/// Closed set of core String identities that can decorate Smithy members.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreIdentity {
    Ipv4Address,
    Ipv6Address,
    Ipv4Cidr,
    Ipv6Cidr,
    Email,
}

impl CoreIdentity {
    pub fn definition(self) -> IdentityDefinition {
        use carina_core::schema::types;
        IdentityDefinition(match self {
            Self::Ipv4Address => types::ipv4_address(),
            Self::Ipv6Address => types::ipv6_address(),
            Self::Ipv4Cidr => types::ipv4_cidr(),
            Self::Ipv6Cidr => types::ipv6_cidr(),
            Self::Email => types::email(),
        })
    }
    pub fn attribute_type(self) -> AttributeType {
        self.definition().into_attribute_type()
    }
}

type Length = Option<(Option<u64>, Option<u64>)>;
type Range = Option<(Option<i64>, Option<i64>)>;

impl IdentityDefinition {
    pub fn string(
        identity: TypeIdentity,
        pattern: Option<PatternSpec>,
        length: Length,
        semantic: Option<CustomValidator>,
        to_dsl: Option<DslTransform>,
    ) -> Self {
        Self(match semantic {
            Some(check) => AttributeType::refined_string_with_validator(
                identity, pattern, length, check, to_dsl,
            ),
            None => AttributeType::refined_string(Some(identity), pattern, length, to_dsl),
        })
    }

    pub fn int(identity: TypeIdentity, range: Range, semantic: Option<CustomValidator>) -> Self {
        Self(match semantic {
            Some(check) => AttributeType::refined_int_with_validator(identity, range, check),
            None => AttributeType::refined_int(Some(identity), range),
        })
    }

    pub fn enum_(
        identity: TypeIdentity,
        pattern: Option<PatternSpec>,
        length: Length,
        values: Option<Vec<String>>,
        aliases: Vec<(String, String)>,
        semantic: Option<CustomValidator>,
        to_dsl: Option<DslTransform>,
    ) -> Self {
        Self::try_enum(identity, pattern, length, values, aliases, semantic, to_dsl)
            .expect("invalid static enum identity definition")
    }

    /// Enum bases are identity definitions, not slots. Check finite values
    /// against the complete identity policy before publishing their exact set.
    pub fn try_enum(
        identity: TypeIdentity,
        pattern: Option<PatternSpec>,
        length: Length,
        values: Option<Vec<String>>,
        aliases: Vec<(String, String)>,
        semantic: Option<CustomValidator>,
        to_dsl: Option<DslTransform>,
    ) -> Result<Self, String> {
        if values.is_none() && (pattern.is_some() || length.is_some()) {
            return Err(format!(
                "{identity}: core does not enforce structural constraints on open Enum bases; use a String identity"
            ));
        }
        let base = AttributeType::refined_string(None, pattern, length, None);
        let validator = crate::MemberStringValidator::new(&base).map_err(|e| e.to_string())?;
        if let Some(values) = &values {
            for value in values {
                if !validator.accepts(value) {
                    return Err(format!(
                        "{identity}: enum value {value:?} violates the identity pattern/length constraints"
                    ));
                }
                if let Some(check) = &semantic {
                    let input: carina_core::resource::Value =
                        carina_core::resource::Value::Concrete(
                            carina_core::resource::ConcreteValue::String(value.as_str().into()),
                        );
                    check(&input).map_err(|e| {
                        format!(
                            "{identity}: enum value {value:?} violates the identity policy: {e}"
                        )
                    })?;
                }
            }
        }
        // Protocol 27 closed enums transport their exact finite set, not a
        // constrained base. Every value has already satisfied that policy.
        Ok(Self(AttributeType::enum_with_base(
            identity,
            AttributeType::string(),
            values,
            aliases,
            semantic,
            to_dsl,
        )))
    }

    pub fn reference(identity: TypeIdentity, pattern: Option<PatternSpec>, length: Length) -> Self {
        Self(AttributeType::reference(identity, pattern, length))
    }

    pub fn into_attribute_type(self) -> AttributeType {
        self.0
    }

    pub fn as_attribute_type(&self) -> &AttributeType {
        &self.0
    }

    pub fn has_validator(&self) -> bool {
        use carina_core::schema::RawShape;
        match self.0.raw_shape() {
            RawShape::String { identity_layer, .. } => identity_layer.validate.is_some(),
            RawShape::Int { identity_layer, .. } => identity_layer.validate.is_some(),
            RawShape::Enum { validate, .. } => validate.is_some(),
            _ => false,
        }
    }
}
