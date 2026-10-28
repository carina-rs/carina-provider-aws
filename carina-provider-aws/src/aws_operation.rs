//! Types that bind AWS fluent builders to their generated resend policy.

use std::future::Future;

use aws_smithy_runtime_api::client::orchestrator::HttpResponse;
use aws_smithy_runtime_api::client::result::SdkError;
use carina_aws_types::Idempotency;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub(crate) const EC2_CREATE_TOKEN_TAG: &str = "carina:create-token";

/// Marker for operations whose builder does not accept a client token.
pub(crate) enum NoToken {}

/// Marker for operations whose builder must receive a stable client token.
pub(crate) enum WithToken {}

/// One invocation's stable AWS idempotency and discovery token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClientToken(String);

impl ClientToken {
    /// Mint a token once at the boundary of a provider invocation.
    pub(crate) fn new() -> Self {
        Self(Uuid::new_v4().simple().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A generated binding between an SDK builder type and its resend semantics.
pub(crate) trait AwsOperation {
    type Resend;
    type Output;
    type Error;

    const NAME: &'static str;
    const IDEMPOTENCY: Idempotency;

    fn send(
        self,
    ) -> impl Future<Output = Result<Self::Output, SdkError<Self::Error, HttpResponse>>> + Send;
}

/// An operation builder with a generated idempotency-token setter.
pub(crate) trait TokenBearing {
    fn with_client_token(self, token: &ClientToken) -> Self;
}

#[cfg(test)]
mod tests {
    use super::ClientToken;

    #[test]
    fn minted_client_token_fits_acm_and_ec2_constraints() {
        let token = ClientToken::new();

        assert_eq!(token.as_str().len(), 32);
        assert!(
            token
                .as_str()
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "token must be lowercase hexadecimal: {}",
            token.as_str()
        );
    }
}
