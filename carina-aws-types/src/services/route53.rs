use carina_core::schema::AttributeType;
use carina_core::schema::{PatternMatch, PatternSpec};

/// AWS DNS-name refinement shared by Route 53 inputs and ACM validation CNAMEs.
/// Labels allow underscores, printable ASCII and octal-escaped bytes; an escape
/// counts as one of the 63 label bytes. The terminal root dot is optional.
/// Route 53's provider validator also checks the 255-byte wire limit and the
/// record-specific wildcard rules. Keep the String anonymous for module inputs.
///
/// The 1024-character carrier bound comes from Route 53's DNSName Smithy shape:
/// <https://docs.aws.amazon.com/Route53/latest/APIReference/API_AliasTarget.html>
/// <https://docs.aws.amazon.com/Route53/latest/DeveloperGuide/DomainNameFormat.html>
/// ACM supplies a DNS name and CNAME target, rather than arbitrary record data:
/// <https://docs.aws.amazon.com/acm/latest/APIReference/API_ResourceRecord.html>
pub fn route53_dns_name() -> AttributeType {
    AttributeType::string_member(
        carina_core::schema::StringIdentityLayer::new(None, None, None, None),
        carina_core::schema::StringMemberConstraints::new(
            Some(PatternSpec::new(
                r"^(?:[\x21-\x2D\x2F-\x5B\x5D-\x7E]|\\[0-3][0-7]{2}){1,63}(?:\.(?:[\x21-\x2D\x2F-\x5B\x5D-\x7E]|\\[0-3][0-7]{2}){1,63})*\.?\z",
                PatternMatch::Full,
            )),
            Some((Some(1), Some(1024))),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use carina_core::resource::{ConcreteValue, Value};
    use carina_core::schema::RawShape;

    #[test]
    fn route53_dns_name_preserves_anonymous_bounds_and_decoded_label_grammar() {
        let attr = route53_dns_name();
        let RawShape::String {
            identity_layer:
                carina_core::schema::StringIdentityParts {
                    identity, validate, ..
                },
            member_layer: carina_core::schema::StringMemberParts { pattern, length },
        } = attr.raw_shape()
        else {
            panic!("DNS String");
        };
        assert!(identity.is_none());
        assert!(pattern.is_some());
        assert_eq!(length, Some((Some(1), Some(1024))));
        assert!(
            validate.is_none(),
            "wire and record rules remain provider-local"
        );
        let dns = crate::tests::rootless(attr);
        for valid in [
            "_token.example.com.",
            "_token.acm-validations.aws.",
            "Example.com",
            "*.example.com.",
            r"\052.example.com.",
            r"a.\052.example.com.",
            r"\101bc.example.com.",
            r"a\056b.example.com.",
            r"\000\040\377.example.com.",
        ] {
            assert!(
                dns.validate_collect(&Value::Concrete(ConcreteValue::String(valid.into())))
                    .is_empty(),
                "{valid:?}"
            );
        }
        for (value, valid) in [
            ("".into(), false),
            ("a..example.com".into(), false),
            ("a b.example.com".into(), false),
            ("example.com.\n".into(), false),
            (r"\400.example.com".into(), false),
            (format!("{}.example.com.", "a".repeat(63)), true),
            (format!("{}.example.com.", "a".repeat(64)), false),
            (format!("{}.example.com.", r"\141".repeat(63)), true),
            (format!("{}.example.com.", r"\141".repeat(64)), false),
            (format!("{}a", "a.".repeat(512)), false),
        ] {
            assert_eq!(
                dns.validate_collect(&Value::Concrete(ConcreteValue::String(
                    value.clone().into()
                )))
                .is_empty(),
                valid,
                "{value:?}"
            );
        }
    }
}
