//! Reusable catalog definitions never contain declaration-local member layers.
use carina_core::schema::{
    AttributeType, CustomValidator, DslTransform, PatternSpec, TypeIdentity,
};

/// A catalog constructor defines an identity, never an attribute slot.
/// There is deliberately no conversion from an arbitrary AttributeType.
/// Providers can construct their own catalog rows with the public factories.
/// Within one provider, core aggregate admission rejects conflicting scalar
/// identity layers; definitions from different provider owners are conjoined.
/// See [`Self::reference`] for the pinned core's Reference agreement limitation.
///
/// ```compile_fail,E0624
/// use carina_aws_types::{AwsType, IdentityDefinition};
/// IdentityDefinition::try_enum(AwsType::DnsRecordType.identity(), None, None,
///     Some(vec!["INVALID".into()]), vec![], None, None);
/// ```
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
        macro_rules! definition {
            ($constructor:path) => {{
                static DEFINITION: std::sync::OnceLock<IdentityDefinition> =
                    std::sync::OnceLock::new();
                DEFINITION
                    .get_or_init(|| IdentityDefinition($constructor()))
                    .clone()
            }};
        }
        match self {
            Self::Ipv4Address => definition!(types::ipv4_address),
            Self::Ipv6Address => definition!(types::ipv6_address),
            Self::Ipv4Cidr => definition!(types::ipv4_cidr),
            Self::Ipv6Cidr => definition!(types::ipv6_cidr),
            Self::Email => definition!(types::email),
        }
    }
    pub fn attribute_type(self) -> AttributeType {
        self.definition().into_attribute_type()
    }
}

type Length = Option<(Option<u64>, Option<u64>)>;
type Range = Option<(Option<i64>, Option<i64>)>;

impl IdentityDefinition {
    /// Define a reusable String identity, without slot-local member constraints.
    /// Core aggregate admission rejects a second differing definition of the
    /// same String identity within a provider. Different owners are conjoined.
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

    /// Define a reusable Int identity, without slot-local member constraints.
    /// Core aggregate admission rejects a second differing definition of the
    /// same Int identity within a provider. Different owners are conjoined.
    pub fn int(identity: TypeIdentity, range: Range, semantic: Option<CustomValidator>) -> Self {
        Self(match semantic {
            Some(check) => AttributeType::refined_int_with_validator(identity, range, check),
            None => AttributeType::refined_int(Some(identity), range),
        })
    }

    /// Define an Enum whose finite values satisfy its complete identity policy.
    /// A provider must use consistent identity definitions; core aggregate
    /// admission rejects conflicting scalar identity layers within that provider.
    /// Definitions from different provider owners are intentionally conjoined.
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
    pub(crate) fn try_enum(
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
        let policy = Self::string(identity.clone(), pattern, length, semantic.clone(), None)
            .into_attribute_type();
        let validator = crate::MemberStringValidator::new(&policy).map_err(|e| e.to_string())?;
        if let Some(values) = &values {
            for value in values {
                if !validator.accepts(value) {
                    return Err(format!(
                        "{identity}: enum value {value:?} violates the identity pattern/length or semantic policy"
                    ));
                }
            }
        }
        for (canonical, _) in &aliases {
            if values
                .as_ref()
                .is_some_and(|values| !values.contains(canonical))
            {
                return Err(format!(
                    "{identity}: alias target {canonical:?} is absent from the enum value set"
                ));
            }
            if !validator.accepts(canonical) {
                return Err(format!(
                    "{identity}: alias target {canonical:?} violates the identity pattern/length or semantic policy"
                ));
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

    /// Define the grammar and length shared by one Reference identity.
    /// A provider must use one consistent definition per identity. Core aggregate
    /// admission rejects conflicting scalar identity layers, but Carina a3412b6
    /// does not yet check Reference pattern/length agreement across declarations.
    /// Definitions from different provider owners are intentionally conjoined.
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
