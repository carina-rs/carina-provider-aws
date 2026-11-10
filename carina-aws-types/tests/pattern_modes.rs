use carina_aws_types::AwsType;
use carina_core::schema::{CompiledPattern, PatternMatch, RawShape};

#[test]
fn catalog_patterns_preserve_accepted_and_rejected_values() {
    for row in AwsType::ALL {
        let attribute = row.attribute_type();
        let pattern = match attribute.raw_shape() {
            RawShape::String {
                identity_layer: carina_core::schema::StringIdentityParts { pattern, .. },
                ..
            }
            | RawShape::Reference { pattern, .. } => pattern,
            _ => None,
        };
        let Some(pattern) = pattern else { continue };
        // These existing expressions have absolute boundaries and no multiline flags.
        assert_eq!(pattern.mode(), &PatternMatch::Full, "{row:?}");
        assert!(pattern.as_str().starts_with('^'), "{row:?}");
        assert!(
            pattern.as_str().ends_with('$') || pattern.as_str().ends_with(r"\z"),
            "{row:?}"
        );
        let compiled = CompiledPattern::try_new(pattern).unwrap();
        assert!(compiled.is_match(row.valid_sample()), "{row:?}");
        assert!(!compiled.is_match("\n"), "{row:?}");
        let previous = regex::Regex::new(pattern.as_str()).unwrap();
        for value in [
            row.valid_sample().to_string(),
            "\n".to_string(),
            format!("prefix{}", row.valid_sample()),
            format!("{}\nsuffix", row.valid_sample()),
        ] {
            assert_eq!(
                compiled.is_match(&value),
                previous.is_match(&value),
                "{row:?}: {value:?}"
            );
        }
    }
}
