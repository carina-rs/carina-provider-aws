use carina_core::provider::ProviderResult;
use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::resource::{Resource, ResourceId};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::ec2_security_group_rules::SecurityGroupRuleSendInput;
use crate::provider::TypedCreateCompletion;
use crate::send_models_generated::{
    Ec2SecurityGroupIngressCreateInput, Ec2SecurityGroupIngressUpdateInput,
};

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
        resource: &Resource<impl Sized>,
        _schema: &ResourceSchema,
    ) -> ProviderResult<TypedCreateCompletion> {
        let input = Ec2SecurityGroupIngressCreateInput::decode(resource)?;
        let sent = input.sent();
        let (
            cidr_ip,
            cidr_ipv6,
            description,
            from_port,
            group_id,
            ip_protocol,
            source_prefix_list_id,
            source_security_group_id,
            source_security_group_owner_id,
            to_port,
        ) = input.into_parts();
        let completion = self
            .create_ec2_security_group_rule(
                resource,
                SecurityGroupRuleSendInput {
                    cidr_ip,
                    cidr_ipv6,
                    description,
                    from_port,
                    group_id,
                    ip_protocol,
                    prefix_list_id: source_prefix_list_id,
                    referenced_security_group_id: source_security_group_id,
                    source_security_group_owner_id,
                    to_port,
                },
                true,
            )
            .await?;
        Ok((sent, completion))
    }

    /// Update an EC2 Security Group Ingress Rule
    pub(crate) async fn update_ec2_security_group_ingress(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        to: Resource,
        _schema: &ResourceSchema,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        let input = Ec2SecurityGroupIngressUpdateInput::decode(&to)?;
        let () = input.into_parts();
        Ok(self.read_ec2_security_group_ingress(&id, identifier).await)
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
