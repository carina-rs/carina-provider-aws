//! Types that bind AWS operation builders to their generated resend policy.

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
}

/// A request whose send implementation and generated resend decision are tied
/// to the same concrete builder type.
pub(crate) trait DecidedSend<C>: AwsOperation {
    fn send(
        self,
        context: C,
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

/// An operation input builder with a generated idempotency-token setter.
pub(crate) trait TokenBearing {
    fn with_client_token(self, token: &ClientToken) -> Self;
}

/// A token-bearing SDK input builder and its guarded client.
///
/// Both fields are deliberately private and cannot be extracted. Request fields
/// can only be changed through [`TokenOperation::configure`], and the input
/// builder has no direct `send` or `customize` method.
#[derive(Clone)]
pub(crate) struct TokenOperation<B, C> {
    input: B,
    client: C,
}

impl<B, C> TokenOperation<B, C> {
    /// Wrap a generated token-bearing input builder and guarded client.
    pub(crate) fn new(input: B, client: C) -> Self {
        Self { input, client }
    }

    /// Configure request fields while keeping the input and client private.
    ///
    /// The closure receives a plain SDK input builder. It can set request
    /// fields, but has no direct `send` or `customize` method; the guarded
    /// client remains inside this wrapper.
    pub(crate) fn configure(self, configure: impl FnOnce(B) -> B) -> Self {
        Self {
            input: configure(self.input),
            client: self.client,
        }
    }
}

impl<B, C> AwsOperation for TokenOperation<B, C>
where
    B: AwsOperation<Resend = WithToken>,
{
    type Resend = WithToken;
    type Output = B::Output;
    type Error = B::Error;

    const NAME: &'static str = B::NAME;
    const IDEMPOTENCY: Idempotency = B::IDEMPOTENCY;
}

impl<B, C> TokenOperation<B, C>
where
    B: AwsOperation<Resend = WithToken> + TokenBearing,
{
    /// Apply the invocation token before invoking a generated concrete sender.
    ///
    /// [`crate::helpers::TokenSendPermit`] can only be constructed by the
    /// token-aware helper entry points.
    pub(crate) fn send_with_token_using<F, Fut>(
        self,
        token: &ClientToken,
        _permit: crate::helpers::TokenSendPermit,
        send: F,
    ) -> Fut
    where
        F: FnOnce(B, C) -> Fut,
        Fut: Future<Output = Result<B::Output, SdkError<B::Error, HttpResponse>>> + Send,
    {
        send(self.input.with_client_token(token), self.client)
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
