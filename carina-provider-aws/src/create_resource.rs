//! Core witnesses accepted by the shared create dispatcher.

use carina_core::executor::normalized::NormalizedResource;
use carina_core::provider::ProviderReadyResource;
use carina_core::resource::{
    AbsoluteBindingAddress, NoPlaceholder, ProviderModuleSource, Resource,
};

mod sealed {
    pub trait Sealed {}
}

/// A resource that has crossed core's desired-normalization boundary.
///
/// This trait is sealed to [`ProviderReadyResource`] for native requests and
/// [`NormalizedResource`] for guest requests. Raw resources cannot implement it.
pub trait AwsCreateResource: sealed::Sealed {
    type Source;
    type Dependencies;

    fn as_resource(
        &self,
    ) -> &Resource<Self::Source, NoPlaceholder, AbsoluteBindingAddress, Self::Dependencies>;
}

impl sealed::Sealed for ProviderReadyResource {}

impl AwsCreateResource for ProviderReadyResource {
    type Source = ProviderModuleSource;
    type Dependencies = ();

    fn as_resource(
        &self,
    ) -> &Resource<Self::Source, NoPlaceholder, AbsoluteBindingAddress, Self::Dependencies> {
        ProviderReadyResource::as_resource(self)
    }
}

impl<S, D> sealed::Sealed for NormalizedResource<S, D> {}

impl<S, D> AwsCreateResource for NormalizedResource<S, D> {
    type Source = S;
    type Dependencies = D;

    fn as_resource(&self) -> &Resource<S, NoPlaceholder, AbsoluteBindingAddress, D> {
        NormalizedResource::as_resource(self)
    }
}
