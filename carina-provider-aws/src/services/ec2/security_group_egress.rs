use carina_core::provider::ProviderResult;
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::resource::{Resource, ResourceId};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::provider::CreateCompletion;

impl AwsProvider {
    /// Read an EC2 Security Group Egress Rule
    pub(crate) async fn read_ec2_security_group_egress(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<ReadOutcome> {
        self.read_ec2_security_group_rule(id, identifier, false)
            .await
    }

    /// Create an EC2 Security Group Egress Rule
    pub(crate) async fn create_ec2_security_group_egress(
        &self,
        resource: &Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<CreateCompletion> {
        self.create_ec2_security_group_rule(resource, schema, false)
            .await
    }

    /// Update an EC2 Security Group Egress Rule
    pub(crate) async fn update_ec2_security_group_egress(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        to: Resource,
        schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        self.update_ec2_security_group_rule(id, identifier, to, schema, false)
            .await
    }

    /// Delete an EC2 Security Group Egress Rule
    pub(crate) async fn delete_ec2_security_group_egress(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        self.delete_ec2_security_group_rule(id, identifier, false)
            .await
    }
}
