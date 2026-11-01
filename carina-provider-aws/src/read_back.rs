//! Provider declarations and identifier-only post-mutation read continuations.

use std::collections::HashMap;
use std::sync::OnceLock;

pub use carina_aws_types::ReadBackPredicate;
use carina_aws_types::Transience;
use carina_core::provider::Provider;
use carina_core::provider::{
    AppliedIdentifier, AppliedState, Observation, ProviderError, ProviderResult, ReadOutcome,
    ReadRequest,
};
use carina_core::resource::{Resource, ResourceId, Value};
use carina_core::schema::ResourceSchema;
use serde::{Deserialize, Serialize};

use crate::AwsProvider;
use crate::provider::CreateCompletion;

/// One schema-checked declaration; applicability belongs to its predicate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadBackRequirement {
    attribute: String,
    predicate: ReadBackPredicate,
}

impl ReadBackRequirement {
    pub fn new(attribute: impl Into<String>, predicate: ReadBackPredicate) -> Self {
        Self {
            attribute: attribute.into(),
            predicate,
        }
    }

    pub fn attribute(&self) -> &str {
        &self.attribute
    }

    fn applies(
        &self,
        resource: &Resource<impl Sized>,
        schema: &ResourceSchema,
    ) -> ProviderResult<bool> {
        match self.predicate {
            ReadBackPredicate::AcmDnsDvo => resource
                .get_enum_attr(schema, self.predicate.input_attribute())
                .map(|value| {
                    value.is_some_and(|value| {
                        aws_sdk_acm::types::ValidationMethod::from(value.api_value())
                            == aws_sdk_acm::types::ValidationMethod::Dns
                    })
                })
                .map_err(|error| crate::helpers::enum_attr_error(&resource.id, error)),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadBackRequirements(Vec<ReadBackRequirement>);

/// Validated DSL attribute roots. A pending read always explains what is missing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<String>", into = "Vec<String>")]
pub struct NonEmptyAttributeNames(Vec<String>);

impl TryFrom<Vec<String>> for NonEmptyAttributeNames {
    type Error = String;
    fn try_from(mut names: Vec<String>) -> Result<Self, Self::Error> {
        if names.is_empty()
            || names.iter().any(|name| {
                name.is_empty()
                    || !name
                        .bytes()
                        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == b'_')
                    || name.as_bytes()[0].is_ascii_digit()
            })
        {
            return Err(
                "read-back missing attributes must be non-empty DSL attribute roots".into(),
            );
        }
        names.sort();
        names.dedup();
        Ok(Self(names))
    }
}
impl From<NonEmptyAttributeNames> for Vec<String> {
    fn from(names: NonEmptyAttributeNames) -> Self {
        names.0
    }
}

/// No request values, observed snapshots, retry counters, or clocks are persisted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadBackContinuation {
    identifier: AppliedIdentifier,
    resource_type: String,
}

impl ReadBackContinuation {
    pub fn identifier(&self) -> &AppliedIdentifier {
        &self.identifier
    }

    pub(crate) fn validate(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        if id.provider != "aws"
            || self.resource_type != id.resource_type.as_str()
            || &self.identifier != identifier
        {
            return Err(ProviderError::internal(
                "read-back continuation does not match its resource or identifier",
            )
            .for_resource(id.clone()));
        }
        Ok(())
    }
}

fn declarations(resource_type: &str) -> &'static [ReadBackRequirement] {
    static DECLARATIONS: OnceLock<HashMap<&'static str, Vec<ReadBackRequirement>>> =
        OnceLock::new();
    DECLARATIONS
        .get_or_init(|| {
            crate::schemas::generated::configs()
                .into_iter()
                .filter(|config| config.schema.kind() == carina_core::schema::SchemaKind::Resource)
                .map(|config| (config.resource_type_name, config.await_on_read_back))
                .collect()
        })
        .get(resource_type)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// Terminal-only output: a final host poll cannot construct another pending step.
#[derive(Debug)]
pub(crate) enum ReadBackCompletion {
    Success(AppliedState),
    Partial {
        identifier: AppliedIdentifier,
        attributes: HashMap<String, Value>,
        reason: ProviderError,
        missing_attributes: NonEmptyAttributeNames,
    },
    Failed {
        observation: Observation,
        error: ProviderError,
    },
}

pub(crate) fn finish_final_read_back(completion: CreateCompletion) -> ReadBackCompletion {
    match completion {
        CreateCompletion::Success(state) => ReadBackCompletion::Success(state),
        CreateCompletion::Partial {
            identifier,
            attributes,
            reason,
            missing_attributes,
        } => ReadBackCompletion::Partial {
            identifier,
            attributes,
            reason,
            missing_attributes,
        },
        CreateCompletion::ReadBackPending {
            continuation,
            attributes,
            reason,
            missing_attributes,
        } => ReadBackCompletion::Partial {
            identifier: continuation.identifier,
            attributes,
            reason,
            missing_attributes,
        },
        CreateCompletion::Failed {
            identifier: _,
            observation,
            error,
        } => ReadBackCompletion::Failed { observation, error },
    }
}

/// The mutation kind controls create-only failure classification, never token state.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ReadBackKind {
    Create,
    Update,
}

pub(crate) fn invalid_read_back(
    id: &ResourceId,
    identifier: AppliedIdentifier,
    error: ProviderError,
) -> CreateCompletion {
    let error = error.for_resource(id.clone());
    CreateCompletion::Failed {
        identifier,
        observation: Observation::ReadFailed(ProviderError::from_wire(error.to_wire())),
        error,
    }
}

impl AwsProvider {
    pub(crate) fn declared_read_back_requirements(
        &self,
        id: &ResourceId,
        attributes: &HashMap<String, Value>,
    ) -> ProviderResult<ReadBackRequirements> {
        let requirements = declarations(id.resource_type.as_str());
        if requirements.is_empty() {
            return Ok(ReadBackRequirements::default());
        }
        let schema = self
            .resource_schema(id.resource_type.as_str())
            .ok_or_else(|| {
                ProviderError::internal("read-back resource schema is absent")
                    .for_resource(id.clone())
            })?;
        let mut resource: Resource = Resource::from_id(id.clone());
        resource.attributes = attributes.clone().into_iter().collect();
        requirements
            .iter()
            .filter_map(|requirement| match requirement.applies(&resource, schema) {
                Ok(true) => Some(Ok(requirement.clone())),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            })
            .collect::<ProviderResult<Vec<_>>>()
            .map(ReadBackRequirements)
    }

    pub(crate) fn classify_read_back(
        &self,
        id: &ResourceId,
        identifier: AppliedIdentifier,
        fallback: HashMap<String, Value>,
        outcome: ProviderResult<ReadOutcome>,
        requirements: ReadBackRequirements,
    ) -> CreateCompletion {
        let permanent_error = outcome.as_ref().is_err_and(|error| {
            !matches!(error, ProviderError::NotFound(_))
                && Transience::of_provider_error(error) == Transience::Permanent
        });
        let (attributes, missing, reason) = match outcome {
            Ok(ReadOutcome::Found { state }) => {
                let missing = requirements
                    .0
                    .iter()
                    .filter(|requirement| !state.attributes().contains_key(requirement.attribute()))
                    .map(|requirement| requirement.attribute.clone())
                    .collect::<Vec<_>>();
                if missing.is_empty() {
                    return CreateCompletion::Success(state);
                }
                let reason = ProviderError::api_error(format!(
                    "post-apply read-back is incomplete: {}",
                    missing.join(", ")
                ));
                let (_, _, attributes) = state.into_parts();
                (attributes, missing, reason)
            }
            other => {
                let reason = match other {
                    Ok(ReadOutcome::NotFound { .. }) => {
                        ProviderError::api_error("post-apply read-back did not find the resource")
                    }
                    Err(error) => {
                        ProviderError::api_error_caused_by("post-apply read-back failed", error)
                    }
                    Ok(ReadOutcome::Found { .. }) => unreachable!(),
                };
                let Some(schema) = self.resource_schema(id.resource_type.as_str()) else {
                    return invalid_read_back(
                        id,
                        identifier,
                        ProviderError::internal("read-back resource schema is absent"),
                    );
                };
                let missing = schema.attributes().keys().cloned().collect();
                (fallback, missing, reason)
            }
        };
        let missing_attributes = match NonEmptyAttributeNames::try_from(missing) {
            Ok(names) => names,
            Err(error) => return invalid_read_back(id, identifier, ProviderError::internal(error)),
        };
        if permanent_error {
            return CreateCompletion::Partial {
                identifier,
                attributes,
                reason,
                missing_attributes,
            };
        }
        CreateCompletion::ReadBackPending {
            continuation: ReadBackContinuation {
                identifier,
                resource_type: id.resource_type.to_string(),
            },
            attributes,
            reason,
            missing_attributes,
        }
    }

    pub(crate) async fn read_back_by_identifier(
        &self,
        resource: &Resource<impl Sized>,
        continuation: &ReadBackContinuation,
        kind: ReadBackKind,
    ) -> CreateCompletion {
        let fallback = resource.resolved_attributes();
        let requirements = match self.declared_read_back_requirements(&resource.id, &fallback) {
            Ok(requirements) => requirements,
            Err(error) => {
                return invalid_read_back(&resource.id, continuation.identifier.clone(), error);
            }
        };
        self.observe_read_back(
            &resource.id,
            continuation.identifier.clone(),
            fallback,
            requirements,
            kind,
        )
        .await
    }

    pub(crate) async fn observe_read_back(
        &self,
        id: &ResourceId,
        identifier: AppliedIdentifier,
        fallback: HashMap<String, Value>,
        requirements: ReadBackRequirements,
        kind: ReadBackKind,
    ) -> CreateCompletion {
        if let Some(requirement) = requirements
            .0
            .iter()
            .find(|requirement| requirement.predicate == ReadBackPredicate::AcmDnsDvo)
        {
            return self
                .read_acm_certificate_back(
                    id,
                    identifier,
                    fallback,
                    requirements.clone(),
                    requirement.attribute(),
                    kind,
                )
                .await;
        }
        let outcome = Provider::read(self, id, &identifier, ReadRequest).await;
        self.classify_read_back(id, identifier, fallback, outcome, requirements)
    }
}

#[cfg(test)]
mod tests {
    use crate::AwsCreateToken;

    #[test]
    fn non_empty_attribute_names_reject_empty_or_invalid_roots() {
        for names in [
            vec![],
            vec![""],
            vec!["DomainValidationOptions"],
            vec!["domain_validation_options[0]"],
            vec!["1attribute"],
        ] {
            assert!(
                super::NonEmptyAttributeNames::try_from(
                    names.into_iter().map(str::to_string).collect::<Vec<_>>()
                )
                .is_err()
            );
        }
        let names = super::NonEmptyAttributeNames::try_from(vec![
            "domain_name".into(),
            "certificate_arn".into(),
            "domain_name".into(),
        ])
        .unwrap();
        assert_eq!(
            Vec::<String>::from(names),
            ["certificate_arn", "domain_name"]
        );
    }

    #[test]
    fn read_back_tokens_require_identifier_and_only_continuation_metadata() {
        let token = serde_json::json!({"phase":"read_back", "identifier":"certificate-arn", "resource_type":"acm.Certificate"});
        let decoded: AwsCreateToken = serde_json::from_value(token.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), token);
        for field in ["identifier", "resource_type"] {
            let mut bad = token.clone();
            bad.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<AwsCreateToken>(bad).is_err(),
                "missing {field}"
            );
        }
        for value in [serde_json::json!(""), serde_json::Value::Null] {
            let mut bad = token.clone();
            bad["identifier"] = value;
            assert!(serde_json::from_value::<AwsCreateToken>(bad).is_err());
        }
        for field in ["request_values", "requirements", "missing"] {
            let mut bad = token.clone();
            bad[field] = serde_json::json!([]);
            assert!(
                serde_json::from_value::<AwsCreateToken>(bad).is_err(),
                "unknown {field}"
            );
        }
    }

    #[test]
    fn saved_token_needs_only_identifier_and_resource_type() {
        let token = serde_json::json!({"phase":"read_back", "identifier":"certificate-arn", "resource_type":"acm.Certificate"});
        let decoded: AwsCreateToken = serde_json::from_value(token.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), token);
    }

    #[test]
    fn acm_dns_predicate_attribute_exists_in_schema() {
        let schema = crate::schemas::generated::acm::certificate::acm_certificate_config().schema;
        assert!(
            schema
                .attributes()
                .contains_key(super::ReadBackPredicate::AcmDnsDvo.input_attribute())
        );
    }

    async fn provider() -> crate::AwsProvider {
        let mock = winterbaume_core::MockAws::builder().build();
        let config = crate::BoundedSdkConfig::for_test(mock.sdk_config("us-east-1").await);
        crate::AwsProvider::from_sdk_config("us-east-1".into(), vec![], vec![], &config)
    }

    #[tokio::test]
    async fn read_back_invalid_schema_returns_typed_failure() {
        use super::ReadBackRequirements;
        use carina_core::provider::{AppliedIdentifier, ReadOutcome};
        use carina_core::resource::Resource;
        let mut provider = provider().await;
        for resource_type in ["test.Unknown", "test.Empty"] {
            if resource_type == "test.Empty" {
                provider.schema_registry.insert(
                    "aws",
                    carina_core::schema::ResourceSchema::new(resource_type)
                        .validate()
                        .unwrap(),
                );
            }
            let resource: Resource = Resource::with_provider("aws", resource_type, "test", None);
            let id = resource.id;
            let completion = provider.classify_read_back(
                &id,
                AppliedIdentifier::new("issued").unwrap(),
                Default::default(),
                Ok(ReadOutcome::not_found(id.clone())),
                ReadBackRequirements::default(),
            );
            assert!(
                matches!(completion, crate::provider::CreateCompletion::Failed { .. }),
                "invalid schema must fail without panicking: {completion:?}"
            );
        }
    }

    #[tokio::test]
    async fn transient_read_back_error_retains_source() {
        use carina_aws_types::Transience;
        use carina_core::provider::{AppliedIdentifier, ProviderError};
        use carina_core::resource::Resource;
        let provider = provider().await;
        let resource: Resource = Resource::with_provider("aws", "s3.Bucket", "test", None);
        let error = ProviderError::api_error_caused_by(
            "GetBucket failed",
            Transience::Transient.caused_by(std::io::Error::other("transport cause")),
        );
        let completion = provider.classify_read_back(
            &resource.id,
            AppliedIdentifier::new("bucket").unwrap(),
            Default::default(),
            Err(error),
            Default::default(),
        );
        let crate::provider::CreateCompletion::ReadBackPending { reason, .. } = completion else {
            panic!("transient read errors stay pending")
        };
        assert!(
            reason.detail().cause().is_some(),
            "read-back reason must retain its provider error: {reason:?}"
        );
        let pending = crate::provider::CreateCompletion::ReadBackPending {
            continuation: super::ReadBackContinuation {
                identifier: AppliedIdentifier::new("bucket").unwrap(),
                resource_type: "s3.Bucket".into(),
            },
            attributes: Default::default(),
            reason,
            missing_attributes: super::NonEmptyAttributeNames::try_from(vec!["bucket".into()])
                .unwrap(),
        };
        let super::ReadBackCompletion::Partial { reason, .. } =
            super::finish_final_read_back(pending)
        else {
            panic!("expected terminal partial")
        };
        assert!(reason.detail().cause().is_some());
        assert!(reason.to_string().contains("transport cause"));
    }

    #[tokio::test]
    async fn local_read_back_errors_finish_partial_with_request_attributes_and_source() {
        use carina_core::provider::{AppliedIdentifier, ProviderError};
        use carina_core::resource::{ConcreteValue, Resource, Value};
        let provider = provider().await;
        let resource: Resource = Resource::with_provider("aws", "s3.Bucket", "test", None);
        let attributes = std::collections::HashMap::from([(
            "bucket".into(),
            Value::Concrete(ConcreteValue::String("bucket".into())),
        )]);
        for error in [
            ProviderError::invalid_input("invalid read model"),
            ProviderError::internal("read model conversion failed"),
            ProviderError::api_error_caused_by(
                "read decode failed",
                std::io::Error::other("throttling text is not a classification"),
            ),
        ] {
            let message = error.to_string();
            let completion = provider.classify_read_back(
                &resource.id,
                AppliedIdentifier::new("bucket").unwrap(),
                attributes.clone(),
                Err(error),
                Default::default(),
            );
            let crate::provider::CreateCompletion::Partial {
                identifier,
                attributes: actual,
                missing_attributes,
                reason,
            } = completion
            else {
                panic!("local read errors must finish partial immediately: {completion:?}")
            };
            assert_eq!(identifier.as_str(), "bucket");
            assert_eq!(actual, attributes);
            let mut expected = provider
                .resource_schema("s3.Bucket")
                .unwrap()
                .attributes()
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            expected.sort();
            assert_eq!(Vec::<String>::from(missing_attributes), expected);
            assert!(reason.detail().cause().is_some());
            assert!(reason.to_string().contains(&message));
        }
    }

    #[test]
    fn predicate_uses_schema_enum_resolution() {
        use super::{ReadBackPredicate, ReadBackRequirement};
        use carina_core::resource::{ConcreteValue, Value};
        let requirement =
            ReadBackRequirement::new("domain_validation_options", ReadBackPredicate::AcmDnsDvo);
        let attributes = std::collections::HashMap::from([(
            "validation_method".into(),
            Value::Concrete(ConcreteValue::String("dns".into())),
        )]);
        let mut resource: carina_core::resource::Resource =
            carina_core::resource::Resource::with_provider("aws", "acm.Certificate", "test", None);
        resource.attributes = attributes.into_iter().collect();
        let schema = crate::schemas::generated::acm::certificate::acm_certificate_config().schema;
        assert!(
            requirement.applies(&resource, &schema).is_err(),
            "raw DSL aliases must follow schema enum validation"
        );
    }
}
