use carina_core::schema::{AttributeType, StructField};

use crate::enum_with_dsl_aliases;

/// The only value accepted by S3's `ObjectLockEnabled` field.
pub fn s3_object_lock_enabled() -> AttributeType {
    enum_with_dsl_aliases(
        &["Enabled"],
        carina_core::schema::enum_identity(
            "ObjectLockEnabled",
            Some("aws.s3.BucketObjectLockConfiguration"),
        ),
    )
}

/// Object Lock retention mode for a bucket-default rule.
fn s3_object_lock_retention_mode() -> AttributeType {
    enum_with_dsl_aliases(
        &["GOVERNANCE", "COMPLIANCE"],
        carina_core::schema::enum_identity(
            "ObjectLockRetentionMode",
            Some("aws.s3.BucketObjectLockConfiguration.ObjectLockRule.DefaultRetention"),
        ),
    )
}

fn s3_object_lock_retention_period() -> AttributeType {
    // PutObjectLockConfiguration with Days=30 and Years=1 returned MalformedXML.
    // Measured in us-east-1 on 2026-08-14. The days-only and years-only
    // directions were both checked successfully, so model the period as a
    // union whose arms cannot carry the other field.
    AttributeType::union(vec![
        AttributeType::struct_(
            "RetentionDays",
            vec![
                StructField::new("days", AttributeType::int())
                    .with_provider_name("Days")
                    .with_description(
                        "Retention duration in days. Set exactly one of `days` or `years`.",
                    )
                    .required(),
            ],
        ),
        AttributeType::struct_(
            "RetentionYears",
            vec![
                StructField::new("years", AttributeType::int())
                    .with_provider_name("Years")
                    .with_description(
                        "Retention duration in years. Set exactly one of `days` or `years`.",
                    )
                    .required(),
            ],
        ),
    ])
}

fn s3_object_lock_default_retention() -> AttributeType {
    // PutObjectLockConfiguration with Mode but no period returned
    // InvalidRequest; a period but no Mode returned MalformedXML. Measured in
    // us-east-1 on 2026-08-14, with both omission directions checked. Keep
    // both fields required whenever DefaultRetention is present.
    AttributeType::struct_(
        "DefaultRetention",
        vec![
            StructField::new("mode", s3_object_lock_retention_mode())
                .with_provider_name("Mode")
                .with_description("Retention mode applied to new objects in the bucket.")
                .required(),
            StructField::new("retention_period", s3_object_lock_retention_period())
                .with_description("Retention duration; set exactly one of `days` or `years`.")
                .required(),
        ],
    )
}

/// Optional bucket-default Object Lock rule.
pub fn bucket_object_lock_rule() -> AttributeType {
    AttributeType::struct_(
        "ObjectLockRule",
        vec![
            StructField::new("default_retention", s3_object_lock_default_retention())
                .with_provider_name("DefaultRetention")
                .with_description(
                    "Bucket-default retention settings. `retention_period` must contain exactly one of `days` or `years`.",
                )
                .with_block_name("default_retention"),
        ],
    )
}

#[cfg(test)]
mod tests {
    use carina_core::resource::{ConcreteValue, Value};
    use indexmap::IndexMap;

    use super::*;

    fn map(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
        Value::Concrete(ConcreteValue::Map(
            entries
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect::<IndexMap<_, _>>(),
        ))
    }

    fn retention_mode(value: &str) -> Value {
        Value::Concrete(ConcreteValue::enum_identifier(format!(
            "aws.s3.BucketObjectLockConfiguration.ObjectLockRule.DefaultRetention.ObjectLockRetentionMode.{}",
            value.to_ascii_lowercase()
        )))
    }

    fn int(value: i64) -> Value {
        Value::Concrete(ConcreteValue::Int(value))
    }

    fn default_retention(mode: Option<&str>, retention_period: Option<Value>) -> Value {
        let mut fields = Vec::new();
        if let Some(mode) = mode {
            fields.push(("mode", retention_mode(mode)));
        }
        if let Some(retention_period) = retention_period {
            fields.push(("retention_period", retention_period));
        }
        map(fields)
    }

    fn rule(default_retention: Value) -> Value {
        map([("default_retention", default_retention)])
    }

    #[test]
    fn object_lock_rule_accepts_days_retention() {
        let value = rule(default_retention(
            Some("GOVERNANCE"),
            Some(map([("days", int(30))])),
        ));

        let schema = carina_core::schema::Schema::flat(bucket_object_lock_rule());
        assert!(schema.validate_collect(&value).is_empty());
    }

    #[test]
    fn object_lock_rule_accepts_years_retention() {
        let value = rule(default_retention(
            Some("COMPLIANCE"),
            Some(map([("years", int(1))])),
        ));

        let schema = carina_core::schema::Schema::flat(bucket_object_lock_rule());
        assert!(schema.validate_collect(&value).is_empty());
    }

    #[test]
    fn object_lock_rule_rejects_days_and_years_together() {
        let schema = carina_core::schema::Schema::flat(bucket_object_lock_rule());
        let both = rule(default_retention(
            Some("GOVERNANCE"),
            Some(map([("days", int(30)), ("years", int(1))])),
        ));
        let days_only = rule(default_retention(
            Some("GOVERNANCE"),
            Some(map([("days", int(30))])),
        ));
        let years_only = rule(default_retention(
            Some("GOVERNANCE"),
            Some(map([("years", int(1))])),
        ));

        let errors = schema.validate_collect(&both);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(matches!(
            &errors[0].1,
            carina_core::schema::TypeError::UnknownStructField {
                struct_name,
                field,
                ..
            } if struct_name == "RetentionDays" && field == "years"
        ));
        assert!(schema.validate_collect(&days_only).is_empty());
        assert!(schema.validate_collect(&years_only).is_empty());
    }

    #[test]
    fn object_lock_rule_empty_period_reports_current_first_union_arm_error() {
        let value = rule(default_retention(Some("GOVERNANCE"), Some(map([]))));

        let schema = carina_core::schema::Schema::flat(bucket_object_lock_rule());
        let errors = schema.validate_collect(&value);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(
            errors[0].1.to_string(),
            "Required attribute 'days' is missing"
        );
    }

    #[test]
    fn object_lock_rule_rejects_mode_without_retention_period() {
        let value = rule(default_retention(Some("GOVERNANCE"), None));

        let schema = carina_core::schema::Schema::flat(bucket_object_lock_rule());
        assert!(!schema.validate_collect(&value).is_empty());
    }

    #[test]
    fn object_lock_rule_rejects_retention_period_without_mode() {
        let value = rule(default_retention(None, Some(map([("days", int(30))]))));

        let schema = carina_core::schema::Schema::flat(bucket_object_lock_rule());
        assert!(!schema.validate_collect(&value).is_empty());
    }

    #[test]
    fn object_lock_rule_allows_default_retention_to_be_omitted() {
        let schema = carina_core::schema::Schema::flat(bucket_object_lock_rule());
        assert!(schema.validate_collect(&map([])).is_empty());
    }
}
