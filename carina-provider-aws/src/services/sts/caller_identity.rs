use std::collections::HashMap;

use carina_core::provider::{BoxFuture, ProviderResult};
use carina_core::resource::{ConcreteValue, DataSource, State, Value};

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;

impl AwsProvider {
    /// Read STS caller identity (data source).
    ///
    /// Calls STS `GetCallerIdentity` and returns `account_id`, `arn`,
    /// `user_id`. Wired into `DataSourceLookups` via
    /// `data_source_lookups::DataSourceLookups`.
    pub(crate) fn do_read_sts_caller_identity_data_source(
        &self,
        resource: &DataSource<impl Clone + Send + Sync + 'static>,
    ) -> BoxFuture<'_, ProviderResult<State>> {
        let id = resource.id.clone();
        Box::pin(async move {
            let response = self
                .sts_client
                .get_caller_identity()
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("Failed to get STS caller identity", e)
                        .for_resource(id.clone())
                })?;

            let mut attributes = HashMap::new();
            if let Some(account) = response.account() {
                attributes.insert(
                    "account_id".to_string(),
                    Value::Concrete(ConcreteValue::String(account.to_string().into())),
                );
            }
            if let Some(arn) = response.arn() {
                attributes.insert(
                    "arn".to_string(),
                    Value::Concrete(ConcreteValue::String(arn.to_string().into())),
                );
            }
            if let Some(user_id) = response.user_id() {
                attributes.insert(
                    "user_id".to_string(),
                    Value::Concrete(ConcreteValue::String(user_id.to_string().into())),
                );
            }

            Ok(State::existing(id, attributes))
        })
    }
}
