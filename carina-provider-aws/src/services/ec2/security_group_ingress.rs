use carina_core::provider::ProviderResult;
use carina_core::provider::{AppliedIdentifier, CreateOutcome, ReadOutcome};
use carina_core::resource::{Resource, ResourceId};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;

impl AwsProvider {
    /// Read an EC2 Security Group Ingress Rule
    pub(crate) async fn read_ec2_security_group_ingress(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        self.read_ec2_security_group_rule(id, identifier, true)
            .await
    }

    /// Create an EC2 Security Group Ingress Rule
    pub(crate) async fn create_ec2_security_group_ingress(
        &self,
        resource: &Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<CreateOutcome> {
        self.create_ec2_security_group_rule(resource, schema, true)
            .await
    }

    /// Update an EC2 Security Group Ingress Rule
    pub(crate) async fn update_ec2_security_group_ingress(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        to: Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        self.update_ec2_security_group_rule(id, identifier, to, schema, true)
            .await
    }

    /// Delete an EC2 Security Group Ingress Rule
    pub(crate) async fn delete_ec2_security_group_ingress(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        self.delete_ec2_security_group_rule(id, identifier, true)
            .await
    }
}
