//! `DataSourceLookups` impl on `AwsProvider`.
//!
//! Each method delegates to a `do_read_<name>_data_source` inherent
//! method that lives next to the rest of that service's hand-written
//! code (under `services/<service>/<resource>.rs`). Splitting it this
//! way keeps Rust's "one impl Trait for Type" rule happy while letting
//! the lookup logic stay near its owning service.

use carina_core::provider::{BoxFuture, ProviderResult};
use carina_core::resource::{Resource, State};

use crate::AwsProvider;
use crate::provider_generated::DataSourceLookups;

impl DataSourceLookups for AwsProvider {
    fn read_sts_caller_identity_data_source(
        &self,
        resource: &Resource,
    ) -> BoxFuture<'_, ProviderResult<State>> {
        self.do_read_sts_caller_identity_data_source(resource)
    }

    fn read_identitystore_user_data_source(
        &self,
        resource: &Resource,
    ) -> BoxFuture<'_, ProviderResult<State>> {
        self.do_read_identitystore_user_data_source(resource)
    }

    fn read_s3_bucket_data_source(
        &self,
        resource: &Resource,
    ) -> BoxFuture<'_, ProviderResult<State>> {
        self.do_read_s3_bucket_data_source(resource)
    }

    fn read_iam_roles_data_source(
        &self,
        resource: &Resource,
    ) -> BoxFuture<'_, ProviderResult<State>> {
        self.do_read_iam_roles_data_source(resource)
    }
}

#[cfg(test)]
mod tests {
    /// Compile-time assertion: `AwsProvider` implements every method
    /// declared on the codegen-generated `DataSourceLookups` trait.
    /// If `carina-codegen-aws` adds a new `DataSourceDef`, this fails
    /// until the corresponding `do_read_..._data_source` is wired.
    #[test]
    fn aws_provider_implements_all_data_source_lookups() {
        fn assert_impl<T: crate::provider_generated::DataSourceLookups>() {}
        assert_impl::<crate::AwsProvider>();
    }
}
