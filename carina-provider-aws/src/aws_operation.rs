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

/// Operation identity and resend semantics carried by a decided SDK error type.
///
/// Generated implementations make it impossible to attach the wrong service or
/// operation label while converting an SDK error into a provider error.
pub(crate) trait DecidedOperationError {
    const NAME: &'static str;
    const IDEMPOTENCY: Idempotency;
}

/// An operation builder with a generated idempotency-token setter.
pub(crate) trait TokenBearing {
    fn with_client_token(self, token: &ClientToken) -> Self;
}

/// A token-bearing SDK builder that cannot be sent without the provider's
/// invocation token.
///
/// The inner builder is deliberately private and cannot be extracted. Request
/// fields can only be changed through [`TokenOperation::configure`].
#[derive(Clone)]
pub(crate) struct TokenOperation<B> {
    inner: B,
}

impl<B> TokenOperation<B> {
    /// Wrap a generated token-bearing builder.
    pub(crate) fn new(inner: B) -> Self {
        Self { inner }
    }

    /// Configure request fields while keeping the builder inside this wrapper.
    pub(crate) fn configure(self, configure: impl FnOnce(B) -> B) -> Self {
        Self {
            inner: configure(self.inner),
        }
    }
}

impl<B> TokenOperation<B>
where
    B: AwsOperation<Resend = WithToken> + TokenBearing,
{
    /// Apply the invocation token and send the inner SDK builder.
    ///
    /// [`crate::helpers::TokenSendPermit`] can only be constructed by the
    /// token-aware helper entry points.
    pub(crate) async fn send_with_token(
        self,
        token: &ClientToken,
        _permit: crate::helpers::TokenSendPermit,
    ) -> Result<B::Output, SdkError<B::Error, HttpResponse>> {
        AwsOperation::send(self.inner.with_client_token(token)).await
    }
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
