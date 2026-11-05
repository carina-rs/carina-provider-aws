use carina_core::schema::{AttributeType, StructField};

// CORSRule and ManageCorsUsing: IDs <=255 characters, closed methods, one wildcard.
// AllowedMethods and AllowedOrigins are required lists with at least one member.
// https://docs.aws.amazon.com/AmazonS3/latest/API/API_CORSRule.html
fn s3_cors_rule() -> AttributeType {
    AttributeType::struct_(
        "CorsRule".to_string(),
        vec![
            StructField::optional("id", super::rule_id())
                .with_provider_name("ID")
                .build(),
            StructField::required(
                "allowed_methods",
                super::nonempty_list(crate::enum_with_dsl_aliases(
                    &["GET", "PUT", "HEAD", "POST", "DELETE"],
                    carina_core::schema::enum_identity(
                        "HttpMethod",
                        Some("aws.s3.BucketCorsConfiguration"),
                    ),
                )),
            )
            .with_provider_name("AllowedMethods")
            .build(),
            StructField::required(
                "allowed_origins",
                super::nonempty_list(AttributeType::refined_string(
                    None,
                    Some(r"^[^*]*(?:\*[^*]*)?\z".to_string()),
                    None,
                    None,
                )),
            )
            .with_provider_name("AllowedOrigins")
            .build(),
            StructField::optional(
                "allowed_headers",
                AttributeType::list(AttributeType::refined_string(
                    None,
                    Some(r"^[^*]*(?:\*[^*]*)?\z".to_string()),
                    None,
                    None,
                )),
            )
            .with_provider_name("AllowedHeaders")
            .build(),
            StructField::optional(
                "expose_headers",
                AttributeType::list(AttributeType::string()),
            )
            .with_provider_name("ExposeHeaders")
            .build(),
            StructField::optional("max_age_seconds", crate::smithy_integer())
                .with_provider_name("MaxAgeSeconds")
                .build(),
        ],
    )
}

// CORSConfiguration.CORSRules is required; retain the existing maximum of 100.
// https://docs.aws.amazon.com/AmazonS3/latest/API/API_CORSConfiguration.html
pub fn bucket_cors_rules() -> AttributeType {
    super::rule_list(s3_cors_rule(), 1, 100)
}
