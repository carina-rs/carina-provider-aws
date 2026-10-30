use carina_core::provider::ProviderResult;
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::resource::{Resource, ResourceId};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::ec2_security_group_rules::SecurityGroupRuleSendInput;
use crate::provider::TypedCreateCompletion;
use crate::send_models_generated::{
    Ec2SecurityGroupEgressCreateInput, Ec2SecurityGroupEgressUpdateInput,
};

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
        resource: &Resource<impl Sized>,
        _schema: &ResourceSchema,
    ) -> ProviderResult<TypedCreateCompletion> {
        let input = Ec2SecurityGroupEgressCreateInput::decode(resource)?;
        let sent = input.sent();
        let crate::send_models_generated::Ec2SecurityGroupEgressCreateParts {
            cidr_ip,
            cidr_ipv6,
            description,
            destination_prefix_list_id,
            destination_security_group_id,
            from_port,
            group_id,
            ip_protocol,
            source_security_group_owner_id,
            to_port,
        } = input.into_parts();
        let cidr_ip = cidr_ip.into_inner();
        let cidr_ipv6 = cidr_ipv6.into_inner();
        let description = description.into_inner();
        let destination_prefix_list_id = destination_prefix_list_id.into_inner();
        let destination_security_group_id = destination_security_group_id.into_inner();
        let from_port = from_port.into_inner();
        let group_id = group_id.into_inner();
        let ip_protocol = ip_protocol.into_inner();
        let source_security_group_owner_id = source_security_group_owner_id.into_inner();
        let to_port = to_port.into_inner();
        let completion = self
            .create_ec2_security_group_rule(
                resource,
                SecurityGroupRuleSendInput {
                    cidr_ip,
                    cidr_ipv6,
                    description,
                    from_port,
                    group_id: Some(group_id),
                    ip_protocol,
                    prefix_list_id: destination_prefix_list_id,
                    referenced_security_group_id: destination_security_group_id,
                    source_security_group_owner_id,
                    to_port,
                },
                false,
            )
            .await?;
        Ok((sent, completion))
    }

    /// Update an EC2 Security Group Egress Rule
    pub(crate) async fn update_ec2_security_group_egress(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        to: Resource,
        _schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let input = Ec2SecurityGroupEgressUpdateInput::decode(&to)?;
        let crate::send_models_generated::Ec2SecurityGroupEgressUpdateParts = input.into_parts();
        Ok(self.read_ec2_security_group_egress(&id, identifier).await)
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
