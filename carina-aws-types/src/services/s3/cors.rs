use carina_core::schema::{AttributeType, StructField};

fn s3_cors_rule() -> AttributeType {
    AttributeType::struct_(
        "CorsRule".to_string(),
        vec![
            StructField::new("id", AttributeType::string()).with_provider_name("ID"),
            StructField::new(
                "allowed_methods",
                AttributeType::list(AttributeType::string()),
            )
            .with_provider_name("AllowedMethods")
            .required(),
            StructField::new(
                "allowed_origins",
                AttributeType::list(AttributeType::string()),
            )
            .with_provider_name("AllowedOrigins")
            .required(),
            StructField::new(
                "allowed_headers",
                AttributeType::list(AttributeType::string()),
            )
            .with_provider_name("AllowedHeaders"),
            StructField::new(
                "expose_headers",
                AttributeType::list(AttributeType::string()),
            )
            .with_provider_name("ExposeHeaders"),
            StructField::new("max_age_seconds", AttributeType::int())
                .with_provider_name("MaxAgeSeconds"),
        ],
    )
}

pub fn bucket_cors_rules() -> AttributeType {
    AttributeType::list(s3_cors_rule())
}
