//! Exercise the AWS component's desired normalization through the real host.
//!
//! Build locally with `CARGO_PROFILE_DEV_DEBUG=0 cargo build -p carina-provider-aws
//! --target wasm32-wasip2`; large DWARF sections can exceed the host's module-size
//! limit. CI selects its release component through `CARINA_AWS_WASM`.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use carina_core::parser::{ExpandedInferredFile, LoadedProviderContext, ProviderConfig};
use carina_core::paths::ResolvedPath;
use carina_core::provider::{BoxFuture, ProviderError, ProviderFactory};
use carina_core::resource::{ConcreteValue, DeferredValue, ProviderResource, Resource, Value};
use carina_plugin_host::{
    AwsCredentialError, AwsCredentialProvisioner, AwsCredentialSource, AwsCredentials,
    WasmProviderFactory,
};
use carina_provider_resolver::{
    LockFile, ProviderLoadIdentity, ProviderTrustClass, VerifiedEgressDeclaration, VerifiedLoad,
};
use indexmap::IndexMap;

fn string(value: &str) -> Value {
    Value::Concrete(ConcreteValue::String(value.into()))
}

fn secret(value: Value) -> Value {
    Value::Deferred(DeferredValue::Secret(Box::new(value)))
}

fn reference(attribute: &str) -> Value {
    Value::resource_ref("peer", attribute, vec![]).resolve_root_for_test()
}

fn map(fields: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Concrete(ConcreteValue::Map(
        fields
            .into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect(),
    ))
}

struct TestCredentials;

impl AwsCredentialSource for TestCredentials {
    fn current(&self) -> BoxFuture<'_, Result<AwsCredentials, AwsCredentialError>> {
        Box::pin(async {
            Ok(AwsCredentials::new(
                "test-access-key",
                "test-secret-key",
                None,
                None,
            ))
        })
    }
}

impl AwsCredentialProvisioner for TestCredentials {
    fn provision(&self) -> BoxFuture<'_, Result<Arc<dyn AwsCredentialSource>, AwsCredentialError>> {
        Box::pin(async { Ok(Arc::new(TestCredentials) as Arc<dyn AwsCredentialSource>) })
    }
}

fn test_egress(
    load: &VerifiedLoad,
    directory: &std::path::Path,
) -> carina_provider_resolver::LockedEgress {
    let declaration = VerifiedEgressDeclaration::read(load).expect("guest egress declaration");
    let canonical_source = declaration.source().to_string();
    let source = canonical_source
        .strip_prefix("file://")
        .map(|path| format!("file://{}", directory.join(path).display()))
        .unwrap_or(canonical_source);
    let hosts = declaration
        .patterns()
        .iter()
        .map(|pattern| format!("    {:?},", pattern.to_string()))
        .collect::<Vec<_>>()
        .join("\n");
    let lock = format!(
        "version = 5\n\n[[provider]]\nname = \"aws\"\nsource = {source:?}\nmode = \"file\"\nsha256 = {:?}\n\n[provider.egress]\nconsent = \"explicit\"\nhosts = [\n{hosts}\n]\n",
        declaration.sha256().as_str(),
    );
    std::fs::write(directory.join("carina-providers.lock"), lock).expect("write test consent");
    let project_dir = ResolvedPath::new(directory).expect("resolve test directory");
    let (lock, _) = LockFile::load(&project_dir)
        .expect("read test consent")
        .expect("test consent file")
        .into_parts();
    lock.locked_egress(&declaration, &project_dir)
        .expect("test consent matches guest")
}

async fn guest_normalizer() -> (
    Box<dyn carina_core::provider::ProviderNormalizer>,
    tempfile::TempDir,
) {
    let path = std::env::var_os("CARINA_AWS_WASM")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../target/wasm32-wasip2/debug/carina-provider-aws.wasm")
        });
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!("read AWS guest {}: {error}; build with CARGO_PROFILE_DEV_DEBUG=0 cargo build -p carina-provider-aws --target wasm32-wasip2", path.display())
    });
    let cache = tempfile::tempdir().expect("component cache directory");
    let load = VerifiedLoad::for_test(
        bytes,
        ProviderLoadIdentity::for_test("aws", ProviderTrustClass::Aws),
    );
    let egress = test_egress(&load, cache.path());
    // The runtime normalizer requires consent; all credentials are synthetic.
    let factory = WasmProviderFactory::from_bytes_with_cache_dir_and_egress(
        load,
        cache.path(),
        Some(&egress),
        &TestCredentials,
    )
    .await
    .expect("load the built AWS component");
    let mut parsed = ExpandedInferredFile::default();
    parsed.providers = vec![ProviderConfig {
        name: "aws".into(),
        attributes: IndexMap::from([("region".into(), string("us-east-1"))]),
        default_tags: IndexMap::new(),
        source: None,
        version: None,
        revision: None,
        unresolved_attributes: IndexMap::new(),
        binding: None,
    }];
    let configs = carina_core::validation::deferred_types::with_session_for_test_async(
        &LoadedProviderContext::empty_for_test(),
        async |type_checks| {
            carina_core::parser::resolve_provider_attributes_with_remote(
                &parsed,
                &HashMap::new(),
                type_checks,
            )
            .expect("resolve local provider configuration")
        },
    )
    .await;
    let resolved_config = configs.iter().next().expect("local provider configuration");
    let config = carina_core::executor::prepare_provider_ready_config(&factory, resolved_config)
        .expect("prepare provider configuration");
    let normalizer = factory
        .create_normalizer(None, &config)
        .await
        .expect("initialize the real AWS guest normalizer without AWS calls");
    (normalizer, cache)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the built wasm32-wasip2 AWS guest; run explicitly after cargo build"]
async fn wasm_guest_enforces_ec2_rules_and_preserves_deferred_leaves() {
    let (normalizer, _cache) = guest_normalizer().await;
    let mut invalid: Vec<ProviderResource> = vec![
        Resource::with_provider("aws", "ec2.FlowLog", "transit-interval", None)
            .with_attribute("resource_type", string("TransitGateway"))
            .with_attribute(
                "max_aggregation_interval",
                Value::Concrete(ConcreteValue::Int(600)),
            )
            .with_attribute("log_destination_type", string("s3"))
            .into(),
    ];
    let error = normalizer
        .normalize_desired(&mut invalid)
        .await
        .expect_err("transit gateway intervals must be checked by the guest");
    assert!(
        error.to_string().contains("max_aggregation_interval"),
        "{error}"
    );
    let mut invalid: Vec<ProviderResource> = vec![
        Resource::with_provider("aws", "ec2.Subnet", "ipv6-hostname", None)
            .with_attribute("ipv6_native", Value::Concrete(ConcreteValue::Bool(true)))
            .with_attribute(
                "private_dns_name_options_on_launch",
                map([("hostname_type", string("ip-name"))]),
            )
            .into(),
    ];
    let error = normalizer
        .normalize_desired(&mut invalid)
        .await
        .expect_err("IPv6-only hostnames must be checked by the guest");
    assert!(error.to_string().contains("hostname_type"), "{error}");
    let mut invalid: Vec<ProviderResource> = vec![
        Resource::with_provider("aws", "ec2.Vpc", "host-tenancy", None)
            .with_attribute("instance_tenancy", string("host"))
            .into(),
    ];
    let error = normalizer
        .normalize_desired(&mut invalid)
        .await
        .expect_err("request-only tenancy restrictions must be checked by the guest");
    assert!(error.to_string().contains("instance_tenancy"), "{error}");

    let interval = secret(Value::Concrete(ConcreteValue::Int(600)));
    let traffic = reference("traffic_type");
    let mut resources: Vec<ProviderResource> = vec![
        Resource::with_provider("aws", "ec2.FlowLog", "unknown", None)
            .with_attribute("resource_type", string("TransitGateway"))
            .with_attribute("log_destination_type", string("s3"))
            .with_attribute("max_aggregation_interval", interval.clone())
            .with_attribute("traffic_type", traffic.clone())
            .into(),
        Resource::with_provider("awscc", "ec2.Vpc", "other-provider", None)
            .with_attribute("instance_tenancy", string("host"))
            .into(),
    ];
    normalizer
        .normalize_desired(&mut resources)
        .await
        .expect("deferred values and the other provider must pass unchanged");
    assert_eq!(
        resources[0].get_attr("max_aggregation_interval"),
        Some(&interval)
    );
    assert_eq!(resources[0].get_attr("traffic_type"), Some(&traffic));
    assert_eq!(
        resources[1].get_attr("instance_tenancy"),
        Some(&string("host"))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the built wasm32-wasip2 AWS guest; run explicitly after cargo build"]
async fn wasm_guest_normalizes_mixed_leaves_restores_deferred_values_and_maps_errors() {
    let (normalizer, _cache) = guest_normalizer().await;

    let private_san = secret(string("PRIVATE.EXAMPLE.COM."));
    let san_ref = reference("domain_name");
    let certificate = Resource::with_provider("aws", "acm.Certificate", "mixed-sans", None)
        .with_attribute("domain_name", string("Example.COM."))
        .with_attribute(
            "subject_alternative_names",
            Value::Concrete(ConcreteValue::List(vec![
                string("EXAMPLE.COM."),
                private_san.clone(),
                string("WWW.EXAMPLE.COM.."),
                san_ref.clone(),
                string("API.EXAMPLE.COM."),
            ])),
        );
    let zone_ref = reference("hosted_zone_id");
    let health = secret(Value::Concrete(ConcreteValue::Bool(false)));
    let record = Resource::with_provider("aws", "route53.RecordSet", "mixed-alias", None)
        .with_attribute("name", string("www.example.com."))
        .with_attribute(
            "alias_target",
            map([
                ("dns_name", string("lb.example.com.")),
                ("hosted_zone_id", zone_ref.clone()),
                ("evaluate_target_health", health.clone()),
            ]),
        );
    let mut resources: Vec<ProviderResource> = vec![certificate.into(), record.into()];
    normalizer
        .normalize_desired(&mut resources)
        .await
        .expect("normalize through host projection, guest view and host restoration");

    assert_eq!(
        resources[0].get_attr("subject_alternative_names"),
        Some(&Value::Concrete(ConcreteValue::List(vec![
            private_san,
            string("www.example.com"),
            san_ref,
            string("api.example.com"),
        ])))
    );
    assert_eq!(
        resources[1].get_attr("name"),
        Some(&string("www.example.com"))
    );
    assert_eq!(
        resources[1].get_attr("alias_target"),
        Some(&map([
            ("dns_name", string("lb.example.com")),
            ("hosted_zone_id", zone_ref),
            ("evaluate_target_health", health),
        ]))
    );

    let mut invalid: Vec<ProviderResource> =
        vec![Resource::with_provider("aws", "ec2.NatGateway", "missing-subnet", None).into()];
    let original = invalid[0].attributes.clone();
    let error = normalizer
        .normalize_desired(&mut invalid)
        .await
        .expect_err("guest validation error must cross the protocol boundary");
    assert!(
        matches!(&error, ProviderError::InvalidInput(_)),
        "{error:?}"
    );
    let wire_error = error.to_wire();
    assert_eq!(
        wire_error
            .resource_id
            .as_ref()
            .expect("error resource")
            .identity,
        "missing-subnet"
    );
    assert!(
        wire_error.message.contains("subnet_id is required"),
        "{wire_error:?}"
    );
    assert_eq!(invalid[0].attributes, original);

    let normalized = resources
        .iter()
        .map(|resource| resource.attributes.clone())
        .collect::<Vec<_>>();
    normalizer
        .normalize_desired(&mut resources)
        .await
        .expect("a guest input error must not poison subsequent normalization");
    assert_eq!(
        resources
            .iter()
            .map(|resource| resource.attributes.clone())
            .collect::<Vec<_>>(),
        normalized
    );
}
