use carina_core::schema::{AttributeType, StructField};

use crate::enum_with_dsl_aliases;

pub fn s3_index_document() -> AttributeType {
    AttributeType::struct_(
        "IndexDocument".to_string(),
        vec![
            StructField::new("suffix", AttributeType::string())
                .with_provider_name("Suffix")
                .required(),
        ],
    )
}

pub fn s3_error_document() -> AttributeType {
    AttributeType::struct_(
        "ErrorDocument".to_string(),
        vec![
            StructField::new("key", AttributeType::string())
                .with_provider_name("Key")
                .required(),
        ],
    )
}

pub fn s3_redirect_all_requests_to() -> AttributeType {
    AttributeType::struct_(
        "RedirectAllRequestsTo".to_string(),
        vec![
            StructField::new("host_name", AttributeType::string())
                .with_provider_name("HostName")
                .required(),
            StructField::new(
                "protocol",
                enum_with_dsl_aliases(
                    &["http", "https"],
                    carina_core::schema::enum_identity(
                        "Protocol",
                        Some("aws.s3.BucketWebsiteConfiguration.RedirectAllRequestsTo"),
                    ),
                ),
            )
            .with_provider_name("Protocol"),
        ],
    )
}
