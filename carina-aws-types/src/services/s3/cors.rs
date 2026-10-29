use carina_core::schema::{AttributeType, StructField};

fn s3_cors_rule() -> AttributeType {
    AttributeType::struct_(
        "CorsRule".to_string(),
        vec![
            StructField::optional("id", AttributeType::string())
                .with_provider_name("ID")
                .build(),
            StructField::required(
                "allowed_methods",
                AttributeType::list(AttributeType::string()),
            )
            .with_provider_name("AllowedMethods")
            .build(),
            StructField::required(
                "allowed_origins",
                AttributeType::list(AttributeType::string()),
            )
            .with_provider_name("AllowedOrigins")
            .build(),
            StructField::optional(
                "allowed_headers",
                AttributeType::list(AttributeType::string()),
            )
            .with_provider_name("AllowedHeaders")
            .build(),
            StructField::optional(
                "expose_headers",
                AttributeType::list(AttributeType::string()),
            )
            .with_provider_name("ExposeHeaders")
            .build(),
            StructField::optional("max_age_seconds", AttributeType::int())
                .with_provider_name("MaxAgeSeconds")
                .build(),
        ],
    )
}

pub fn bucket_cors_rules() -> AttributeType {
    AttributeType::list(s3_cors_rule())
}
