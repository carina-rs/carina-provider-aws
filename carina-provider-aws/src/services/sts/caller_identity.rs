use carina_core::provider::{BoxFuture, ProviderResult};
use carina_core::resource::{ConcreteValue, DataSource, State, Value};

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::read_model::found_data_source_read;
use crate::read_models_generated::StsCallerIdentityDataSourceReadModel;

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

            let string_value =
                |value: &str| Value::Concrete(ConcreteValue::String(value.to_string().into()));
            found_data_source_read(
                &id,
                None,
                StsCallerIdentityDataSourceReadModel {
                    account_id: response.account().map(string_value),
                    arn: response.arn().map(string_value),
                    user_id: response.user_id().map(string_value),
                },
            )
        })
    }
}
