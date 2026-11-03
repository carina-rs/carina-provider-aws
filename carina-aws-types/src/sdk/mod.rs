//! SDK types and service handles whose constructors always install the resend guard.
//!
//! Each service is enabled by its matching Cargo feature. No services are enabled
//! by default. Features never change how a client is guarded.
//!
//! Only non-client SDK modules are re-exported. Handles have no `Deref`, raw
//! client constructor, or conversion from a service configuration. Each forbidden
//! operation below has a positive control that validates the rest of its snippet.
//!
#![cfg_attr(
    feature = "s3",
    doc = r#"
A handle does not expose the raw client through `Deref`:

```compile_fail
use carina_aws_types::sdk::s3::S3Client;
fn require_deref<T: std::ops::Deref>() {}
require_deref::<S3Client>();
```

The same trait bound works for an allowed wrapper:

```
use carina_aws_types::sdk::s3::S3Client;
fn require_deref<T: std::ops::Deref>() {}
require_deref::<Box<S3Client>>();
```

The raw constructor cannot be named on the handle:

```compile_fail
use carina_aws_types::sdk::s3::S3Client;
let _ = S3Client::from_conf;
```

Its guarded factory can be named, without constructing a runtime client:

```
use carina_aws_types::sdk::s3::S3Client;
let _ = S3Client::new;
```

A borrowed client configuration cannot build another client:

```compile_fail
use carina_aws_types::sdk::s3::S3Client;
fn rebuild(client: &S3Client) {
    let _ = client.inner().config().to_builder().build_client();
}
```

The same chain can build an allowed configuration value:

```
use carina_aws_types::sdk::s3::S3Client;
fn rebuild(client: &S3Client) {
    let _ = client.inner().config().to_builder().build();
}
```

A configuration cannot be converted into an inferred client:

```compile_fail
use carina_aws_types::sdk::s3::S3Client;
fn same_type<T>(_: &T, value: T) -> T { value }
fn rebuild(client: &S3Client) {
    let mut config = client.inner().config().to_builder();
    config.set_interceptors([]);
    let existing = client.inner().clone();
    let _ = same_type(&existing, config.build().into());
}
```

The same conversion is valid when the inferred type is a configuration:

```
use carina_aws_types::sdk::s3::S3Client;
fn same_type<T>(_: &T, value: T) -> T { value }
fn rebuild(client: &S3Client) {
    let mut config = client.inner().config().to_builder();
    config.set_interceptors([]);
    let existing = client.inner().config().clone();
    let _ = same_type(&existing, config.build().into());
}
```
"#
)]

// Keep every service's visibility and construction guarantees identical.
#[allow(unused_macros, reason = "no service features may be enabled")]
macro_rules! guarded_service {
    ($service:ident, $sdk:ident, $client:ident $(, $configure:path)?) => {
        #[doc = concat!(
            "SDK types and a guarded ", stringify!($service), " handle.\n\n",
            "```compile_fail\nuse carina_aws_types::sdk::", stringify!($service), "::Client;\n```\n\n",
            "The guarded handle is available:\n\n",
            "```\nuse carina_aws_types::sdk::", stringify!($service), "::", stringify!($client), ";\n```\n\n",
            "```compile_fail\nuse carina_aws_types::sdk::", stringify!($service), "::Config;\n```\n\n",
            "Shared configuration is available from aws-types:\n\n",
            "```\nuse aws_types::SdkConfig;\n```\n\n",
            "```compile_fail\nuse carina_aws_types::sdk::", stringify!($service), "::config;\n```\n\n",
            "The non-client types module is available:\n\n",
            "```\nuse carina_aws_types::sdk::", stringify!($service), "::types;\n```"
        )]
        pub mod $service {
            pub use $sdk::{error, operation, primitives, types};

            /// A service client that always carries [`crate::ResendGuard`].
            #[derive(Clone, Debug)]
            pub struct $client {
                inner: $sdk::Client,
            }

            impl $client {
                /// Construct a guarded client using the service's idempotency lookup.
                /// Credentials, transport, timeouts, and retry bounds are retained.
                pub fn new(
                    config: &aws_types::SdkConfig,
                    lookup: fn(&str) -> crate::Idempotency,
                ) -> Self {
                    let builder = $sdk::config::Builder::from(config);
                    $(let builder = $configure(builder);)?
                    let mut builder = builder;
                    builder.push_interceptor(
                        aws_smithy_runtime_api::client::interceptors::SharedInterceptor::permanent(
                            crate::ResendGuard::new(lookup),
                        ),
                    );
                    Self {
                        inner: $sdk::Client::from_conf(builder.build()),
                    }
                }

                /// Borrow the guarded SDK client for fluent operations and `send_with`.
                /// Cloning it retains the guard; its type has no exported name here.
                pub fn inner(&self) -> &$sdk::Client {
                    &self.inner
                }
            }

            #[cfg(test)]
            #[test]
            fn installs_permanent_resend_guard() {
                use aws_smithy_runtime_api::client::interceptors::Intercept;

                let config = super::tests::sdk_config();
                let client = $client::new(&config, |_| crate::Idempotency::NotIdempotent);
                let guard = client.inner().config().interceptors()
                    .find(|interceptor| interceptor.name() == "carina.ResendGuard")
                    .expect("every service handle must install ResendGuard");
                assert!(format!("{guard:?}").contains("permanent: true"));
            }
        }
    };
}

#[cfg(feature = "s3")]
fn configure_s3(builder: aws_sdk_s3::config::Builder) -> aws_sdk_s3::config::Builder {
    // Express session auth creates an internal CreateSession client without our guard.
    // The rejecting provider also blocks that client if an operation flips the flag.
    builder
        .disable_s3_express_session_auth(true)
        .express_credentials_provider(aws_credential_types::credential_fn::provide_credentials_fn(
            || async {
                Err(
                    aws_credential_types::provider::error::CredentialsError::invalid_configuration(
                        "S3 Express session auth is not supported",
                    ),
                )
            },
        ))
}

#[cfg(feature = "account")]
guarded_service!(account, aws_sdk_account, AccountClient);
#[cfg(feature = "acm")]
guarded_service!(acm, aws_sdk_acm, AcmClient);
#[cfg(feature = "cloudcontrol")]
guarded_service!(cloudcontrol, aws_sdk_cloudcontrol, CloudControlClient);
#[cfg(feature = "cloudwatchlogs")]
guarded_service!(cloudwatchlogs, aws_sdk_cloudwatchlogs, CloudWatchLogsClient);
#[cfg(feature = "ec2")]
guarded_service!(ec2, aws_sdk_ec2, Ec2Client);
#[cfg(feature = "iam")]
guarded_service!(iam, aws_sdk_iam, IamClient);
#[cfg(feature = "identitystore")]
guarded_service!(identitystore, aws_sdk_identitystore, IdentityStoreClient);
#[cfg(feature = "kms")]
guarded_service!(kms, aws_sdk_kms, KmsClient);
#[cfg(feature = "organizations")]
guarded_service!(organizations, aws_sdk_organizations, OrganizationsClient);
#[cfg(feature = "route53")]
guarded_service!(route53, aws_sdk_route53, Route53Client);
#[cfg(feature = "s3")]
guarded_service!(s3, aws_sdk_s3, S3Client, super::configure_s3);
#[cfg(feature = "sqs")]
guarded_service!(sqs, aws_sdk_sqs, SqsClient);
#[cfg(feature = "sts")]
guarded_service!(sts, aws_sdk_sts, StsClient);

#[cfg(test)]
mod tests;
