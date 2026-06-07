//! acm.Certificate schema definition for AWS Cloud Control
//!
//! Auto-generated from Smithy model: com.amazonaws.acm
//!
//! DO NOT EDIT MANUALLY - regenerate with smithy-codegen

use super::AwsSchemaConfig;
use super::tags_type;
use super::validate_tags_map;
use carina_core::schema::{AttributeSchema, AttributeType, ResourceSchema, StructField, types};

const VALID_CERTIFICATE_TRANSPARENCY_LOGGING_PREFERENCE: &[&str] =
    &["DISABLED", "ENABLED", "disabled", "enabled"];

const VALID_EXPORT: &[&str] = &["DISABLED", "ENABLED", "disabled", "enabled"];

const VALID_KEY_ALGORITHM: &[&str] = &[
    "EC_prime256v1",
    "EC_secp384r1",
    "EC_secp521r1",
    "RSA_1024",
    "RSA_2048",
    "RSA_3072",
    "RSA_4096",
    "ec_prime256v1",
    "ec_secp384r1",
    "ec_secp521r1",
    "rsa_1024",
    "rsa_2048",
    "rsa_3072",
    "rsa_4096",
];

const VALID_RENEWAL_ELIGIBILITY: &[&str] = &["ELIGIBLE", "INELIGIBLE", "eligible", "ineligible"];

const VALID_RENEWAL_STATUS: &[&str] = &[
    "FAILED",
    "PENDING_AUTO_RENEWAL",
    "PENDING_VALIDATION",
    "SUCCESS",
    "failed",
    "pending_auto_renewal",
    "pending_validation",
    "success",
];

const VALID_RENEWAL_STATUS_REASON: &[&str] = &[
    "ADDITIONAL_VERIFICATION_REQUIRED",
    "CAA_ERROR",
    "DOMAIN_NOT_ALLOWED",
    "DOMAIN_VALIDATION_DENIED",
    "INVALID_PUBLIC_DOMAIN",
    "NO_AVAILABLE_CONTACTS",
    "OTHER",
    "PCA_ACCESS_DENIED",
    "PCA_INVALID_ARGS",
    "PCA_INVALID_ARN",
    "PCA_INVALID_DURATION",
    "PCA_INVALID_STATE",
    "PCA_LIMIT_EXCEEDED",
    "PCA_NAME_CONSTRAINTS_VALIDATION",
    "PCA_REQUEST_FAILED",
    "PCA_RESOURCE_NOT_FOUND",
    "SLR_NOT_FOUND",
    "additional_verification_required",
    "caa_error",
    "domain_not_allowed",
    "domain_validation_denied",
    "invalid_public_domain",
    "no_available_contacts",
    "other",
    "pca_access_denied",
    "pca_invalid_args",
    "pca_invalid_arn",
    "pca_invalid_duration",
    "pca_invalid_state",
    "pca_limit_exceeded",
    "pca_name_constraints_validation",
    "pca_request_failed",
    "pca_resource_not_found",
    "slr_not_found",
];

const VALID_STATUS: &[&str] = &[
    "EXPIRED",
    "FAILED",
    "INACTIVE",
    "ISSUED",
    "PENDING_VALIDATION",
    "REVOKED",
    "VALIDATION_TIMED_OUT",
    "expired",
    "failed",
    "inactive",
    "issued",
    "pending_validation",
    "revoked",
    "validation_timed_out",
];

const VALID_TYPE: &[&str] = &["CNAME", "cname"];

const VALID_VALIDATION_METHOD: &[&str] = &["DNS", "EMAIL", "HTTP", "dns", "email", "http"];

const VALID_VALIDATION_STATUS: &[&str] = &[
    "FAILED",
    "PENDING_VALIDATION",
    "SUCCESS",
    "failed",
    "pending_validation",
    "success",
];

/// Returns the schema config for acm.Certificate (Smithy: com.amazonaws.acm)
pub fn acm_certificate_config() -> AwsSchemaConfig {
    AwsSchemaConfig {
        aws_type_name: "AWS::CertificateManager::Certificate",
        resource_type_name: "acm.Certificate",
        has_tags: true,
        schema: ResourceSchema::new("acm.Certificate")
        .with_description("Contains metadata about an ACM certificate. This structure is returned in the response to a DescribeCertificate request.")
        .attribute(
            AttributeSchema::new("domain_name", AttributeType::string())
                .required()
                .create_only()
                .with_description("Fully qualified domain name (FQDN), such as www.example.com, that you want to secure with an ACM certificate. Use an asterisk (*) to create a wildcard...")
                .with_provider_name("DomainName"),
        )
        .attribute(
            AttributeSchema::new("idempotency_token", AttributeType::string())
                .create_only()
                .with_description("Customer chosen string that can be used to distinguish between calls to RequestCertificate. Idempotency tokens time out after one hour. Therefore, if ...")
                .with_provider_name("IdempotencyToken"),
        )
        .attribute(
            AttributeSchema::new("key_algorithm", AttributeType::enum_(carina_core::schema::enum_identity("KeyAlgorithm", Some("aws.acm.Certificate")), Some(vec!["EC_prime256v1".to_string(), "EC_secp384r1".to_string(), "EC_secp521r1".to_string(), "RSA_1024".to_string(), "RSA_2048".to_string(), "RSA_3072".to_string(), "RSA_4096".to_string()]), vec![("EC_prime256v1".to_string(), "ec_prime256v1".to_string()), ("EC_secp384r1".to_string(), "ec_secp384r1".to_string()), ("EC_secp521r1".to_string(), "ec_secp521r1".to_string()), ("RSA_1024".to_string(), "rsa_1024".to_string()), ("RSA_2048".to_string(), "rsa_2048".to_string()), ("RSA_3072".to_string(), "rsa_3072".to_string()), ("RSA_4096".to_string(), "rsa_4096".to_string())], None, None))
                .create_only()
                .with_description("Specifies the algorithm of the public and private key pair that your certificate uses to encrypt data. RSA is the default key algorithm for ACM certif...")
                .with_provider_name("KeyAlgorithm"),
        )
        .attribute(
            AttributeSchema::new("options", AttributeType::struct_(
                    "CertificateOptions".to_string(),
                    vec![
                    StructField::new("certificate_transparency_logging_preference", AttributeType::enum_(carina_core::schema::enum_identity("CertificateTransparencyLoggingPreference", Some("aws.acm.Certificate.CertificateOptions")), Some(vec!["DISABLED".to_string(), "ENABLED".to_string()]), vec![("DISABLED".to_string(), "disabled".to_string()), ("ENABLED".to_string(), "enabled".to_string())], None, None)).with_description("You can opt out of certificate transparency logging by specifying the DISABLED option. Opt in by specifying ENABLED.").with_provider_name("CertificateTransparencyLoggingPreference"),
                    StructField::new("export", AttributeType::enum_(carina_core::schema::enum_identity("Export", Some("aws.acm.Certificate.CertificateOptions")), Some(vec!["DISABLED".to_string(), "ENABLED".to_string()]), vec![("DISABLED".to_string(), "disabled".to_string()), ("ENABLED".to_string(), "enabled".to_string())], None, None)).with_description("You can opt in to allow the export of your certificates by specifying ENABLED. You cannot update the value of Export after the the certificate is crea...").with_provider_name("Export")
                    ],
                ))
                .create_only()
                .with_description("You can use this parameter to specify whether to add the certificate to a certificate transparency log and export your certificate. Certificate transp...")
                .with_provider_name("Options"),
        )
        .attribute(
            AttributeSchema::new("subject_alternative_names", AttributeType::list(AttributeType::string()))
                .create_only()
                .with_description("Additional FQDNs to be included in the Subject Alternative Name extension of the ACM certificate. For example, add the name www.example.net to a certi...")
                .with_provider_name("SubjectAlternativeNames"),
        )
        .attribute(
            AttributeSchema::new("validation_method", AttributeType::enum_(carina_core::schema::enum_identity("ValidationMethod", Some("aws.acm.Certificate")), Some(vec!["DNS".to_string(), "EMAIL".to_string(), "HTTP".to_string()]), vec![("DNS".to_string(), "dns".to_string()), ("EMAIL".to_string(), "email".to_string()), ("HTTP".to_string(), "http".to_string())], None, None))
                .create_only()
                .with_description("The method you want to use if you are requesting a public certificate to validate that you own or control domain. You can validate with DNS or validat...")
                .with_provider_name("ValidationMethod"),
        )
        .attribute(
            AttributeSchema::new("certificate_arn", super::arn())
                .read_only()
                .with_description("The Amazon Resource Name (ARN) of the certificate. For more information about ARNs, see Amazon Resource Names (ARNs) in the Amazon Web Services Genera... (read-only)")
                .with_provider_name("CertificateArn"),
        )
        .attribute(
            AttributeSchema::new("domain_validation_options", AttributeType::list(AttributeType::struct_(
                    "DomainValidation".to_string(),
                    vec![
                    StructField::new("domain_name", AttributeType::string()).required().with_description("A fully qualified domain name (FQDN) in the certificate. For example, www.example.com or example.com.").with_provider_name("DomainName"),
                    StructField::new("http_redirect", AttributeType::struct_(
                    "HttpRedirect".to_string(),
                    vec![
                    StructField::new("redirect_from", AttributeType::string()).with_description("The URL including the domain to be validated. The certificate authority sends GET requests here during validation.").with_provider_name("RedirectFrom"),
                    StructField::new("redirect_to", AttributeType::string()).with_description("The URL hosting the validation token. RedirectFrom must return this content or redirect here.").with_provider_name("RedirectTo")
                    ],
                )).with_description("Contains information for HTTP-based domain validation of certificates requested through Amazon CloudFront and issued by ACM. This field exists only wh...").with_provider_name("HttpRedirect"),
                    StructField::new("resource_record", AttributeType::struct_(
                    "ResourceRecord".to_string(),
                    vec![
                    StructField::new("name", AttributeType::string()).required().with_description("The name of the DNS record to create in your domain. This is supplied by ACM.").with_provider_name("Name"),
                    StructField::new("type", AttributeType::enum_(carina_core::schema::enum_identity("Type", Some("aws.acm.Certificate.DomainValidation.ResourceRecord")), Some(vec!["CNAME".to_string()]), vec![("CNAME".to_string(), "cname".to_string())], None, None)).required().with_description("The type of DNS record. Currently this can be CNAME.").with_provider_name("Type"),
                    StructField::new("value", AttributeType::string()).required().with_description("The value of the CNAME record to add to your DNS database. This is supplied by ACM.").with_provider_name("Value")
                    ],
                )).deferred_populate().with_description("Contains the CNAME record that you add to your DNS database for domain validation. For more information, see Use DNS to Validate Domain Ownership. The...").with_provider_name("ResourceRecord"),
                    StructField::new("validation_domain", AttributeType::string()).with_description("The domain name that ACM used to send domain validation emails.").with_provider_name("ValidationDomain"),
                    StructField::new("validation_emails", AttributeType::list(types::email())).with_description("A list of email addresses that ACM used to send domain validation emails.").with_provider_name("ValidationEmails"),
                    StructField::new("validation_method", AttributeType::enum_(carina_core::schema::enum_identity("ValidationMethod", Some("aws.acm.Certificate.DomainValidation")), Some(vec!["DNS".to_string(), "EMAIL".to_string(), "HTTP".to_string()]), vec![("DNS".to_string(), "dns".to_string()), ("EMAIL".to_string(), "email".to_string()), ("HTTP".to_string(), "http".to_string())], None, None)).with_description("Specifies the domain validation method.").with_provider_name("ValidationMethod"),
                    StructField::new("validation_status", AttributeType::enum_(carina_core::schema::enum_identity("ValidationStatus", Some("aws.acm.Certificate.DomainValidation")), Some(vec!["FAILED".to_string(), "PENDING_VALIDATION".to_string(), "SUCCESS".to_string()]), vec![("FAILED".to_string(), "failed".to_string()), ("PENDING_VALIDATION".to_string(), "pending_validation".to_string()), ("SUCCESS".to_string(), "success".to_string())], None, None)).with_description("The validation status of the domain name. This can be one of the following values: PENDING_VALIDATION SUCCESS FAILED").with_provider_name("ValidationStatus")
                    ],
                )))
                .read_only()
                .with_description("Contains information about the initial validation of each domain name that occurs as a result of the RequestCertificate request. This field exists onl... (read-only)")
                .with_provider_name("DomainValidationOptions"),
        )
        .attribute(
            AttributeSchema::new("renewal_eligibility", AttributeType::enum_(carina_core::schema::enum_identity("RenewalEligibility", Some("aws.acm.Certificate")), Some(vec!["ELIGIBLE".to_string(), "INELIGIBLE".to_string()]), vec![("ELIGIBLE".to_string(), "eligible".to_string()), ("INELIGIBLE".to_string(), "ineligible".to_string())], None, None))
                .read_only()
                .with_description("Specifies whether the certificate is eligible for renewal. At this time, only exported private certificates can be renewed with the RenewCertificate c... (read-only)")
                .with_provider_name("RenewalEligibility"),
        )
        .attribute(
            AttributeSchema::new("renewal_summary", AttributeType::struct_(
                    "RenewalSummary".to_string(),
                    vec![
                    StructField::new("domain_validation_options", AttributeType::list(AttributeType::struct_(
                    "DomainValidation".to_string(),
                    vec![
                    StructField::new("domain_name", AttributeType::string()).required().with_description("A fully qualified domain name (FQDN) in the certificate. For example, www.example.com or example.com.").with_provider_name("DomainName"),
                    StructField::new("http_redirect", AttributeType::struct_(
                    "HttpRedirect".to_string(),
                    vec![
                    StructField::new("redirect_from", AttributeType::string()).with_description("The URL including the domain to be validated. The certificate authority sends GET requests here during validation.").with_provider_name("RedirectFrom"),
                    StructField::new("redirect_to", AttributeType::string()).with_description("The URL hosting the validation token. RedirectFrom must return this content or redirect here.").with_provider_name("RedirectTo")
                    ],
                )).with_description("Contains information for HTTP-based domain validation of certificates requested through Amazon CloudFront and issued by ACM. This field exists only wh...").with_provider_name("HttpRedirect"),
                    StructField::new("resource_record", AttributeType::struct_(
                    "ResourceRecord".to_string(),
                    vec![
                    StructField::new("name", AttributeType::string()).required().with_description("The name of the DNS record to create in your domain. This is supplied by ACM.").with_provider_name("Name"),
                    StructField::new("type", AttributeType::enum_(carina_core::schema::enum_identity("Type", Some("aws.acm.Certificate.RenewalSummary.DomainValidation.ResourceRecord")), Some(vec!["CNAME".to_string()]), vec![("CNAME".to_string(), "cname".to_string())], None, None)).required().with_description("The type of DNS record. Currently this can be CNAME.").with_provider_name("Type"),
                    StructField::new("value", AttributeType::string()).required().with_description("The value of the CNAME record to add to your DNS database. This is supplied by ACM.").with_provider_name("Value")
                    ],
                )).with_description("Contains the CNAME record that you add to your DNS database for domain validation. For more information, see Use DNS to Validate Domain Ownership. The...").with_provider_name("ResourceRecord"),
                    StructField::new("validation_domain", AttributeType::string()).with_description("The domain name that ACM used to send domain validation emails.").with_provider_name("ValidationDomain"),
                    StructField::new("validation_emails", AttributeType::list(types::email())).with_description("A list of email addresses that ACM used to send domain validation emails.").with_provider_name("ValidationEmails"),
                    StructField::new("validation_method", AttributeType::enum_(carina_core::schema::enum_identity("ValidationMethod", Some("aws.acm.Certificate.RenewalSummary.DomainValidation")), Some(vec!["DNS".to_string(), "EMAIL".to_string(), "HTTP".to_string()]), vec![("DNS".to_string(), "dns".to_string()), ("EMAIL".to_string(), "email".to_string()), ("HTTP".to_string(), "http".to_string())], None, None)).with_description("Specifies the domain validation method.").with_provider_name("ValidationMethod"),
                    StructField::new("validation_status", AttributeType::enum_(carina_core::schema::enum_identity("ValidationStatus", Some("aws.acm.Certificate.RenewalSummary.DomainValidation")), Some(vec!["FAILED".to_string(), "PENDING_VALIDATION".to_string(), "SUCCESS".to_string()]), vec![("FAILED".to_string(), "failed".to_string()), ("PENDING_VALIDATION".to_string(), "pending_validation".to_string()), ("SUCCESS".to_string(), "success".to_string())], None, None)).with_description("The validation status of the domain name. This can be one of the following values: PENDING_VALIDATION SUCCESS FAILED").with_provider_name("ValidationStatus")
                    ],
                ))).required().with_description("Contains information about the validation of each domain name in the certificate, as it pertains to ACM's managed renewal. This is different from the ...").with_provider_name("DomainValidationOptions"),
                    StructField::new("renewal_status", AttributeType::enum_(carina_core::schema::enum_identity("RenewalStatus", Some("aws.acm.Certificate.RenewalSummary")), Some(vec!["FAILED".to_string(), "PENDING_AUTO_RENEWAL".to_string(), "PENDING_VALIDATION".to_string(), "SUCCESS".to_string()]), vec![("FAILED".to_string(), "failed".to_string()), ("PENDING_AUTO_RENEWAL".to_string(), "pending_auto_renewal".to_string()), ("PENDING_VALIDATION".to_string(), "pending_validation".to_string()), ("SUCCESS".to_string(), "success".to_string())], None, None)).required().with_description("The status of ACM's managed renewal of the certificate.").with_provider_name("RenewalStatus"),
                    StructField::new("renewal_status_reason", AttributeType::enum_(carina_core::schema::enum_identity("RenewalStatusReason", Some("aws.acm.Certificate.RenewalSummary")), Some(vec!["ADDITIONAL_VERIFICATION_REQUIRED".to_string(), "CAA_ERROR".to_string(), "DOMAIN_NOT_ALLOWED".to_string(), "DOMAIN_VALIDATION_DENIED".to_string(), "INVALID_PUBLIC_DOMAIN".to_string(), "NO_AVAILABLE_CONTACTS".to_string(), "OTHER".to_string(), "PCA_ACCESS_DENIED".to_string(), "PCA_INVALID_ARGS".to_string(), "PCA_INVALID_ARN".to_string(), "PCA_INVALID_DURATION".to_string(), "PCA_INVALID_STATE".to_string(), "PCA_LIMIT_EXCEEDED".to_string(), "PCA_NAME_CONSTRAINTS_VALIDATION".to_string(), "PCA_REQUEST_FAILED".to_string(), "PCA_RESOURCE_NOT_FOUND".to_string(), "SLR_NOT_FOUND".to_string()]), vec![("ADDITIONAL_VERIFICATION_REQUIRED".to_string(), "additional_verification_required".to_string()), ("CAA_ERROR".to_string(), "caa_error".to_string()), ("DOMAIN_NOT_ALLOWED".to_string(), "domain_not_allowed".to_string()), ("DOMAIN_VALIDATION_DENIED".to_string(), "domain_validation_denied".to_string()), ("INVALID_PUBLIC_DOMAIN".to_string(), "invalid_public_domain".to_string()), ("NO_AVAILABLE_CONTACTS".to_string(), "no_available_contacts".to_string()), ("OTHER".to_string(), "other".to_string()), ("PCA_ACCESS_DENIED".to_string(), "pca_access_denied".to_string()), ("PCA_INVALID_ARGS".to_string(), "pca_invalid_args".to_string()), ("PCA_INVALID_ARN".to_string(), "pca_invalid_arn".to_string()), ("PCA_INVALID_DURATION".to_string(), "pca_invalid_duration".to_string()), ("PCA_INVALID_STATE".to_string(), "pca_invalid_state".to_string()), ("PCA_LIMIT_EXCEEDED".to_string(), "pca_limit_exceeded".to_string()), ("PCA_NAME_CONSTRAINTS_VALIDATION".to_string(), "pca_name_constraints_validation".to_string()), ("PCA_REQUEST_FAILED".to_string(), "pca_request_failed".to_string()), ("PCA_RESOURCE_NOT_FOUND".to_string(), "pca_resource_not_found".to_string()), ("SLR_NOT_FOUND".to_string(), "slr_not_found".to_string())], None, None)).with_description("The reason that a renewal request was unsuccessful.").with_provider_name("RenewalStatusReason"),
                    StructField::new("updated_at", AttributeType::string()).required().with_description("The time at which the renewal summary was last updated.").with_provider_name("UpdatedAt")
                    ],
                ))
                .read_only()
                .with_description("Contains information about the status of ACM's managed renewal for the certificate. This field exists only when the certificate type is AMAZON_ISSUED. (read-only)")
                .with_provider_name("RenewalSummary"),
        )
        .attribute(
            AttributeSchema::new("status", AttributeType::enum_(carina_core::schema::enum_identity("Status", Some("aws.acm.Certificate")), Some(vec!["EXPIRED".to_string(), "FAILED".to_string(), "INACTIVE".to_string(), "ISSUED".to_string(), "PENDING_VALIDATION".to_string(), "REVOKED".to_string(), "VALIDATION_TIMED_OUT".to_string()]), vec![("EXPIRED".to_string(), "expired".to_string()), ("FAILED".to_string(), "failed".to_string()), ("INACTIVE".to_string(), "inactive".to_string()), ("ISSUED".to_string(), "issued".to_string()), ("PENDING_VALIDATION".to_string(), "pending_validation".to_string()), ("REVOKED".to_string(), "revoked".to_string()), ("VALIDATION_TIMED_OUT".to_string(), "validation_timed_out".to_string())], None, None))
                .read_only()
                .deferred_populate()
                .with_description("The status of the certificate. A certificate enters status PENDING_VALIDATION upon being requested, unless it fails for any of the reasons given in th... (read-only)")
                .with_provider_name("Status"),
        )
        .attribute(
            AttributeSchema::new("type", AttributeType::enum_(carina_core::schema::enum_identity("Type", Some("aws.acm.Certificate")), Some(vec!["AMAZON_ISSUED".to_string(), "IMPORTED".to_string(), "PRIVATE".to_string()]), vec![("AMAZON_ISSUED".to_string(), "amazon_issued".to_string()), ("IMPORTED".to_string(), "imported".to_string()), ("PRIVATE".to_string(), "private".to_string())], None, None))
                .read_only()
                .with_description("The source of the certificate. For certificates provided by ACM, this value is AMAZON_ISSUED. For certificates that you imported with ImportCertificat... (read-only)")
                .with_provider_name("Type"),
        )
        .attribute(
            AttributeSchema::new("tags", tags_type())
                .with_description("The tags for the resource.")
                .with_provider_name("Tags"),
        )
        .with_validator(validate_tags_map)
    }
}

/// Returns the resource type name and all enum valid values for this module
pub fn enum_valid_values() -> (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
) {
    (
        "acm.Certificate",
        &[
            (
                "certificate_transparency_logging_preference",
                VALID_CERTIFICATE_TRANSPARENCY_LOGGING_PREFERENCE,
            ),
            ("export", VALID_EXPORT),
            ("key_algorithm", VALID_KEY_ALGORITHM),
            ("renewal_eligibility", VALID_RENEWAL_ELIGIBILITY),
            ("renewal_status", VALID_RENEWAL_STATUS),
            ("renewal_status_reason", VALID_RENEWAL_STATUS_REASON),
            ("status", VALID_STATUS),
            ("type", VALID_TYPE),
            ("validation_method", VALID_VALIDATION_METHOD),
            ("validation_status", VALID_VALIDATION_STATUS),
        ],
    )
}

/// Maps DSL alias values back to canonical AWS values for this module.
/// e.g., ("ip_protocol", "all") -> Some("-1")
pub fn enum_alias_reverse(attr_name: &str, value: &str) -> Option<&'static str> {
    match (attr_name, value) {
        ("certificate_transparency_logging_preference", "disabled") => Some("DISABLED"),
        ("certificate_transparency_logging_preference", "enabled") => Some("ENABLED"),
        ("export", "disabled") => Some("DISABLED"),
        ("export", "enabled") => Some("ENABLED"),
        ("key_algorithm", "ec_prime256v1") => Some("EC_prime256v1"),
        ("key_algorithm", "ec_secp384r1") => Some("EC_secp384r1"),
        ("key_algorithm", "ec_secp521r1") => Some("EC_secp521r1"),
        ("key_algorithm", "rsa_1024") => Some("RSA_1024"),
        ("key_algorithm", "rsa_2048") => Some("RSA_2048"),
        ("key_algorithm", "rsa_3072") => Some("RSA_3072"),
        ("key_algorithm", "rsa_4096") => Some("RSA_4096"),
        ("renewal_eligibility", "eligible") => Some("ELIGIBLE"),
        ("renewal_eligibility", "ineligible") => Some("INELIGIBLE"),
        ("renewal_status", "failed") => Some("FAILED"),
        ("renewal_status", "pending_auto_renewal") => Some("PENDING_AUTO_RENEWAL"),
        ("renewal_status", "pending_validation") => Some("PENDING_VALIDATION"),
        ("renewal_status", "success") => Some("SUCCESS"),
        ("renewal_status_reason", "additional_verification_required") => {
            Some("ADDITIONAL_VERIFICATION_REQUIRED")
        }
        ("renewal_status_reason", "caa_error") => Some("CAA_ERROR"),
        ("renewal_status_reason", "domain_not_allowed") => Some("DOMAIN_NOT_ALLOWED"),
        ("renewal_status_reason", "domain_validation_denied") => Some("DOMAIN_VALIDATION_DENIED"),
        ("renewal_status_reason", "invalid_public_domain") => Some("INVALID_PUBLIC_DOMAIN"),
        ("renewal_status_reason", "no_available_contacts") => Some("NO_AVAILABLE_CONTACTS"),
        ("renewal_status_reason", "other") => Some("OTHER"),
        ("renewal_status_reason", "pca_access_denied") => Some("PCA_ACCESS_DENIED"),
        ("renewal_status_reason", "pca_invalid_args") => Some("PCA_INVALID_ARGS"),
        ("renewal_status_reason", "pca_invalid_arn") => Some("PCA_INVALID_ARN"),
        ("renewal_status_reason", "pca_invalid_duration") => Some("PCA_INVALID_DURATION"),
        ("renewal_status_reason", "pca_invalid_state") => Some("PCA_INVALID_STATE"),
        ("renewal_status_reason", "pca_limit_exceeded") => Some("PCA_LIMIT_EXCEEDED"),
        ("renewal_status_reason", "pca_name_constraints_validation") => {
            Some("PCA_NAME_CONSTRAINTS_VALIDATION")
        }
        ("renewal_status_reason", "pca_request_failed") => Some("PCA_REQUEST_FAILED"),
        ("renewal_status_reason", "pca_resource_not_found") => Some("PCA_RESOURCE_NOT_FOUND"),
        ("renewal_status_reason", "slr_not_found") => Some("SLR_NOT_FOUND"),
        ("status", "expired") => Some("EXPIRED"),
        ("status", "failed") => Some("FAILED"),
        ("status", "inactive") => Some("INACTIVE"),
        ("status", "issued") => Some("ISSUED"),
        ("status", "pending_validation") => Some("PENDING_VALIDATION"),
        ("status", "revoked") => Some("REVOKED"),
        ("status", "validation_timed_out") => Some("VALIDATION_TIMED_OUT"),
        ("type", "cname") => Some("CNAME"),
        ("validation_method", "dns") => Some("DNS"),
        ("validation_method", "email") => Some("EMAIL"),
        ("validation_method", "http") => Some("HTTP"),
        ("validation_status", "failed") => Some("FAILED"),
        ("validation_status", "pending_validation") => Some("PENDING_VALIDATION"),
        ("validation_status", "success") => Some("SUCCESS"),
        _ => None,
    }
}

/// Returns all enum alias entries as (attr_name, alias, canonical) tuples.
pub fn enum_alias_entries() -> &'static [(&'static str, &'static str, &'static str)] {
    &[
        (
            "certificate_transparency_logging_preference",
            "disabled",
            "DISABLED",
        ),
        (
            "certificate_transparency_logging_preference",
            "enabled",
            "ENABLED",
        ),
        ("export", "disabled", "DISABLED"),
        ("export", "enabled", "ENABLED"),
        ("key_algorithm", "ec_prime256v1", "EC_prime256v1"),
        ("key_algorithm", "ec_secp384r1", "EC_secp384r1"),
        ("key_algorithm", "ec_secp521r1", "EC_secp521r1"),
        ("key_algorithm", "rsa_1024", "RSA_1024"),
        ("key_algorithm", "rsa_2048", "RSA_2048"),
        ("key_algorithm", "rsa_3072", "RSA_3072"),
        ("key_algorithm", "rsa_4096", "RSA_4096"),
        ("renewal_eligibility", "eligible", "ELIGIBLE"),
        ("renewal_eligibility", "ineligible", "INELIGIBLE"),
        ("renewal_status", "failed", "FAILED"),
        (
            "renewal_status",
            "pending_auto_renewal",
            "PENDING_AUTO_RENEWAL",
        ),
        ("renewal_status", "pending_validation", "PENDING_VALIDATION"),
        ("renewal_status", "success", "SUCCESS"),
        (
            "renewal_status_reason",
            "additional_verification_required",
            "ADDITIONAL_VERIFICATION_REQUIRED",
        ),
        ("renewal_status_reason", "caa_error", "CAA_ERROR"),
        (
            "renewal_status_reason",
            "domain_not_allowed",
            "DOMAIN_NOT_ALLOWED",
        ),
        (
            "renewal_status_reason",
            "domain_validation_denied",
            "DOMAIN_VALIDATION_DENIED",
        ),
        (
            "renewal_status_reason",
            "invalid_public_domain",
            "INVALID_PUBLIC_DOMAIN",
        ),
        (
            "renewal_status_reason",
            "no_available_contacts",
            "NO_AVAILABLE_CONTACTS",
        ),
        ("renewal_status_reason", "other", "OTHER"),
        (
            "renewal_status_reason",
            "pca_access_denied",
            "PCA_ACCESS_DENIED",
        ),
        (
            "renewal_status_reason",
            "pca_invalid_args",
            "PCA_INVALID_ARGS",
        ),
        (
            "renewal_status_reason",
            "pca_invalid_arn",
            "PCA_INVALID_ARN",
        ),
        (
            "renewal_status_reason",
            "pca_invalid_duration",
            "PCA_INVALID_DURATION",
        ),
        (
            "renewal_status_reason",
            "pca_invalid_state",
            "PCA_INVALID_STATE",
        ),
        (
            "renewal_status_reason",
            "pca_limit_exceeded",
            "PCA_LIMIT_EXCEEDED",
        ),
        (
            "renewal_status_reason",
            "pca_name_constraints_validation",
            "PCA_NAME_CONSTRAINTS_VALIDATION",
        ),
        (
            "renewal_status_reason",
            "pca_request_failed",
            "PCA_REQUEST_FAILED",
        ),
        (
            "renewal_status_reason",
            "pca_resource_not_found",
            "PCA_RESOURCE_NOT_FOUND",
        ),
        ("renewal_status_reason", "slr_not_found", "SLR_NOT_FOUND"),
        ("status", "expired", "EXPIRED"),
        ("status", "failed", "FAILED"),
        ("status", "inactive", "INACTIVE"),
        ("status", "issued", "ISSUED"),
        ("status", "pending_validation", "PENDING_VALIDATION"),
        ("status", "revoked", "REVOKED"),
        ("status", "validation_timed_out", "VALIDATION_TIMED_OUT"),
        ("type", "cname", "CNAME"),
        ("validation_method", "dns", "DNS"),
        ("validation_method", "email", "EMAIL"),
        ("validation_method", "http", "HTTP"),
        ("validation_status", "failed", "FAILED"),
        (
            "validation_status",
            "pending_validation",
            "PENDING_VALIDATION",
        ),
        ("validation_status", "success", "SUCCESS"),
    ]
}
