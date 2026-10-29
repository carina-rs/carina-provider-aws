//! Auto-generated provider boilerplate
//!
//! DO NOT EDIT MANUALLY - regenerate with:
//!   ./carina-provider-aws/scripts/generate-provider.sh

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use carina_core::provider::{
    AppliedIdentifier, BoxFuture, ProviderError, ProviderResult, ReadOutcome,
};
use carina_core::resource::{ConcreteValue, DataSource, Resource, ResourceId, State, Value};
use indexmap::IndexMap;

// ===== Generated Methods on AwsProvider =====

/// Typed values extracted from the AWS read shape for `ec2.Vpc`.
#[allow(dead_code)]
pub(crate) struct Ec2VpcReadFields {
    pub(crate) cidr_block: Option<Value>,
    pub(crate) instance_tenancy: Option<Value>,
    pub(crate) vpc_id: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `ec2.Subnet`.
#[allow(dead_code)]
pub(crate) struct Ec2SubnetReadFields {
    pub(crate) assign_ipv6_address_on_creation: Option<Value>,
    pub(crate) availability_zone: Option<Value>,
    pub(crate) availability_zone_id: Option<Value>,
    pub(crate) cidr_block: Option<Value>,
    pub(crate) enable_dns64: Option<Value>,
    pub(crate) enable_lni_at_device_index: Option<Value>,
    pub(crate) ipv6_native: Option<Value>,
    pub(crate) map_public_ip_on_launch: Option<Value>,
    pub(crate) outpost_arn: Option<Value>,
    pub(crate) private_dns_name_options_on_launch: Option<Value>,
    pub(crate) subnet_id: Option<Value>,
    pub(crate) vpc_id: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `ec2.InternetGateway`.
#[allow(dead_code)]
pub(crate) struct Ec2InternetGatewayReadFields {
    pub(crate) internet_gateway_id: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `ec2.RouteTable`.
#[allow(dead_code)]
pub(crate) struct Ec2RouteTableReadFields {
    pub(crate) route_table_id: Option<Value>,
    pub(crate) vpc_id: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `ec2.Route`.
#[allow(dead_code)]
pub(crate) struct Ec2RouteReadFields {
    pub(crate) destination_cidr_block: Option<Value>,
    pub(crate) gateway_id: Option<Value>,
    pub(crate) nat_gateway_id: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `ec2.SecurityGroup`.
#[allow(dead_code)]
pub(crate) struct Ec2SecurityGroupReadFields {
    pub(crate) description: Option<Value>,
    pub(crate) group_id: Option<Value>,
    pub(crate) group_name: Option<Value>,
    pub(crate) vpc_id: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `ec2.SecurityGroupIngress`.
#[allow(dead_code)]
pub(crate) struct Ec2SecurityGroupIngressReadFields {
    pub(crate) cidr_ipv6: Option<Value>,
    pub(crate) description: Option<Value>,
    pub(crate) from_port: Option<Value>,
    pub(crate) group_id: Option<Value>,
    pub(crate) ip_protocol: Option<Value>,
    pub(crate) security_group_rule_id: Option<Value>,
    pub(crate) source_prefix_list_id: Option<Value>,
    pub(crate) to_port: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `ec2.SecurityGroupEgress`.
#[allow(dead_code)]
pub(crate) struct Ec2SecurityGroupEgressReadFields {
    pub(crate) cidr_ipv6: Option<Value>,
    pub(crate) description: Option<Value>,
    pub(crate) destination_prefix_list_id: Option<Value>,
    pub(crate) from_port: Option<Value>,
    pub(crate) group_id: Option<Value>,
    pub(crate) ip_protocol: Option<Value>,
    pub(crate) security_group_rule_id: Option<Value>,
    pub(crate) to_port: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `ec2.EgressOnlyInternetGateway`.
#[allow(dead_code)]
pub(crate) struct Ec2EgressOnlyInternetGatewayReadFields {
    pub(crate) egress_only_internet_gateway_id: Option<Value>,
    pub(crate) vpc_id: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `ec2.Eip`.
#[allow(dead_code)]
pub(crate) struct Ec2EipReadFields {
    pub(crate) allocation_id: Option<Value>,
    pub(crate) domain: Option<Value>,
    pub(crate) public_ip: Option<Value>,
    pub(crate) public_ipv4_pool: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `ec2.FlowLog`.
#[allow(dead_code)]
pub(crate) struct Ec2FlowLogReadFields {
    pub(crate) deliver_logs_permission_arn: Option<Value>,
    pub(crate) flow_log_id: Option<Value>,
    pub(crate) log_destination: Option<Value>,
    pub(crate) log_destination_type: Option<Value>,
    pub(crate) log_format: Option<Value>,
    pub(crate) log_group_name: Option<Value>,
    pub(crate) max_aggregation_interval: Option<Value>,
    pub(crate) traffic_type: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `ec2.NatGateway`.
#[allow(dead_code)]
pub(crate) struct Ec2NatGatewayReadFields {
    pub(crate) allocation_id: Option<Value>,
    pub(crate) availability_mode: Option<Value>,
    pub(crate) connectivity_type: Option<Value>,
    pub(crate) nat_gateway_id: Option<Value>,
    pub(crate) subnet_id: Option<Value>,
    pub(crate) vpc_id: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `ec2.SubnetRouteTableAssociation`.
#[allow(dead_code)]
pub(crate) struct Ec2SubnetRouteTableAssociationReadFields {
    pub(crate) public_ipv4_pool: Option<Value>,
    pub(crate) route_table_id: Option<Value>,
    pub(crate) subnet_id: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `ec2.TransitGateway`.
#[allow(dead_code)]
pub(crate) struct Ec2TransitGatewayReadFields {
    pub(crate) description: Option<Value>,
    pub(crate) options: Option<Value>,
    pub(crate) transit_gateway_id: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `ec2.TransitGatewayAttachment`.
#[allow(dead_code)]
pub(crate) struct Ec2TransitGatewayAttachmentReadFields {
    pub(crate) options: Option<Value>,
    pub(crate) subnet_ids: Option<Value>,
    pub(crate) transit_gateway_attachment_id: Option<Value>,
    pub(crate) transit_gateway_id: Option<Value>,
    pub(crate) vpc_id: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `ec2.VpcEndpoint`.
#[allow(dead_code)]
pub(crate) struct Ec2VpcEndpointReadFields {
    pub(crate) policy_document: Option<Value>,
    pub(crate) private_dns_enabled: Option<Value>,
    pub(crate) resource_configuration_arn: Option<Value>,
    pub(crate) route_table_ids: Option<Value>,
    pub(crate) security_group_ids: Option<Value>,
    pub(crate) service_name: Option<Value>,
    pub(crate) service_network_arn: Option<Value>,
    pub(crate) service_region: Option<Value>,
    pub(crate) subnet_ids: Option<Value>,
    pub(crate) vpc_endpoint_id: Option<Value>,
    pub(crate) vpc_endpoint_type: Option<Value>,
    pub(crate) vpc_id: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `ec2.VpcPeeringConnection`.
#[allow(dead_code)]
pub(crate) struct Ec2VpcPeeringConnectionReadFields {
    pub(crate) peer_owner_id: Option<Value>,
    pub(crate) peer_vpc_id: Option<Value>,
    pub(crate) vpc_id: Option<Value>,
    pub(crate) vpc_peering_connection_id: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `ec2.VpnGateway`.
#[allow(dead_code)]
pub(crate) struct Ec2VpnGatewayReadFields {
    pub(crate) amazon_side_asn: Option<Value>,
    pub(crate) availability_zone: Option<Value>,
    pub(crate) r#type: Option<Value>,
    pub(crate) vpn_gateway_id: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `organizations.Organization`.
#[allow(dead_code)]
pub(crate) struct OrganizationsOrganizationReadFields {
    pub(crate) arn: Option<Value>,
    pub(crate) feature_set: Option<Value>,
    pub(crate) id: Option<Value>,
    pub(crate) master_account_arn: Option<Value>,
    pub(crate) master_account_email: Option<Value>,
    pub(crate) master_account_id: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `organizations.Account`.
#[allow(dead_code)]
pub(crate) struct OrganizationsAccountReadFields {
    pub(crate) arn: Option<Value>,
    pub(crate) email: Option<Value>,
    pub(crate) id: Option<Value>,
    pub(crate) joined_method: Option<Value>,
    pub(crate) name: Option<Value>,
    pub(crate) status: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `route53.RecordSet`.
#[allow(dead_code)]
pub(crate) struct Route53RecordSetReadFields {
    pub(crate) name: Option<Value>,
    pub(crate) ttl: Option<Value>,
    pub(crate) r#type: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `iam.Role`.
#[allow(dead_code)]
pub(crate) struct IamRoleReadFields {
    pub(crate) arn: Option<Value>,
    pub(crate) assume_role_policy_document: Option<Value>,
    pub(crate) description: Option<Value>,
    pub(crate) max_session_duration: Option<Value>,
    pub(crate) path: Option<Value>,
    pub(crate) role_id: Option<Value>,
    pub(crate) role_name: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `logs.LogGroup`.
#[allow(dead_code)]
pub(crate) struct LogsLogGroupReadFields {
    pub(crate) deletion_protection_enabled: Option<Value>,
    pub(crate) kms_key_id: Option<Value>,
    pub(crate) log_group_class: Option<Value>,
    pub(crate) log_group_name: Option<Value>,
    pub(crate) retention_in_days: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

/// Typed values extracted from the AWS read shape for `acm.Certificate`.
#[allow(dead_code)]
pub(crate) struct AcmCertificateReadFields {
    pub(crate) certificate_arn: Option<Value>,
    pub(crate) domain_name: Option<Value>,
    pub(crate) key_algorithm: Option<Value>,
    pub(crate) renewal_eligibility: Option<Value>,
    pub(crate) status: Option<Value>,
    pub(crate) subject_alternative_names: Option<Value>,
    pub(crate) r#type: Option<Value>,
    pub(crate) applied_identifier: Option<String>,
}

#[allow(dead_code)]
impl AwsProvider {
    /// Delete ec2.Vpc (generated)
    pub(crate) async fn delete_ec2_vpc(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_vpc()
            .vpc_id(identifier.as_str())
            .send()
            .await
            .map_err(|e| api_error_with_meta("Failed to delete vpc", e).for_resource(id.clone()))?;
        Ok(())
    }

    /// Delete ec2.Subnet (generated)
    pub(crate) async fn delete_ec2_subnet(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_subnet()
            .subnet_id(identifier.as_str())
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete subnet", e).for_resource(id.clone())
            })?;
        Ok(())
    }

    /// Delete ec2.RouteTable (generated)
    pub(crate) async fn delete_ec2_route_table(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_route_table()
            .route_table_id(identifier.as_str())
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete route table", e).for_resource(id.clone())
            })?;
        Ok(())
    }

    /// Delete ec2.SecurityGroup (generated)
    pub(crate) async fn delete_ec2_security_group(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_security_group()
            .group_id(identifier.as_str())
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete security group", e).for_resource(id.clone())
            })?;
        Ok(())
    }

    /// Delete ec2.NatGateway (generated)
    pub(crate) async fn delete_ec2_nat_gateway(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_nat_gateway()
            .nat_gateway_id(identifier.as_str())
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete nat gateway", e).for_resource(id.clone())
            })?;
        Ok(())
    }

    /// Delete ec2.TransitGateway (generated)
    pub(crate) async fn delete_ec2_transit_gateway(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_transit_gateway()
            .transit_gateway_id(identifier.as_str())
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete transit gateway", e).for_resource(id.clone())
            })?;
        Ok(())
    }

    /// Delete ec2.TransitGatewayAttachment (generated)
    pub(crate) async fn delete_ec2_transit_gateway_attachment(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
    ) -> ProviderResult<()> {
        self.ec2_client
            .delete_transit_gateway_vpc_attachment()
            .transit_gateway_attachment_id(identifier.as_str())
            .send()
            .await
            .map_err(|e| {
                api_error_with_meta("Failed to delete transit gateway attachment", e)
                    .for_resource(id.clone())
            })?;
        Ok(())
    }

    /// Update ec2.InternetGateway: apply tag changes and read back (generated)
    pub(crate) async fn update_ec2_internet_gateway(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        to: Resource,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        self.apply_ec2_tags(
            &id,
            identifier.as_str(),
            &to.resolved_attributes(),
            Some(&from.attributes),
        )
        .await?;
        Ok(self.read_ec2_internet_gateway(&id, identifier).await)
    }

    /// Update ec2.RouteTable: apply tag changes and read back (generated)
    pub(crate) async fn update_ec2_route_table(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        to: Resource,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        self.apply_ec2_tags(
            &id,
            identifier.as_str(),
            &to.resolved_attributes(),
            Some(&from.attributes),
        )
        .await?;
        Ok(self.read_ec2_route_table(&id, identifier).await)
    }

    /// Update ec2.SecurityGroup: apply tag changes and read back (generated)
    pub(crate) async fn update_ec2_security_group(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        from: &State,
        to: Resource,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        self.apply_ec2_tags(
            &id,
            identifier.as_str(),
            &to.resolved_attributes(),
            Some(&from.attributes),
        )
        .await?;
        Ok(self.read_ec2_security_group(&id, identifier).await)
    }

    /// Update organizations.Organization (no-op, just read back current state) (generated)
    pub(crate) async fn update_organizations_organization(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        _to: Resource,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        Ok(self.read_organizations_organization(&id, identifier).await)
    }

    /// Extract ec2.Vpc attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_vpc_attributes(obj: &aws_sdk_ec2::types::Vpc) -> Ec2VpcReadFields {
        let cidr_block = obj
            .cidr_block()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let instance_tenancy = obj
            .instance_tenancy()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let vpc_id = obj
            .vpc_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = obj.vpc_id().map(String::from);
        Ec2VpcReadFields {
            cidr_block,
            instance_tenancy,
            vpc_id,
            applied_identifier,
        }
    }

    /// Extract ec2.Subnet attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_subnet_attributes(
        obj: &aws_sdk_ec2::types::Subnet,
    ) -> Ec2SubnetReadFields {
        let assign_ipv6_address_on_creation = obj
            .assign_ipv6_address_on_creation()
            .map(|v| Value::Concrete(ConcreteValue::Bool(v)));
        let availability_zone = obj
            .availability_zone()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let availability_zone_id = obj
            .availability_zone_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let cidr_block = obj
            .cidr_block()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let enable_dns64 = obj
            .enable_dns64()
            .map(|v| Value::Concrete(ConcreteValue::Bool(v)));
        let enable_lni_at_device_index = obj
            .enable_lni_at_device_index()
            .map(|v| Value::Concrete(ConcreteValue::Int(v as i64)));
        let ipv6_native = obj
            .ipv6_native()
            .map(|v| Value::Concrete(ConcreteValue::Bool(v)));
        let map_public_ip_on_launch = obj
            .map_public_ip_on_launch()
            .map(|v| Value::Concrete(ConcreteValue::Bool(v)));
        let outpost_arn = obj
            .outpost_arn()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let subnet_id = obj
            .subnet_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let vpc_id = obj
            .vpc_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let private_dns_name_options_on_launch =
            obj.private_dns_name_options_on_launch()
                .and_then(|dns_opts| {
                    let mut fields = IndexMap::new();
                    if let Some(v) = dns_opts.hostname_type() {
                        fields.insert(
                            "hostname_type".to_string(),
                            Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                        );
                    }
                    if let Some(v) = dns_opts.enable_resource_name_dns_a_record() {
                        fields.insert(
                            "enable_resource_name_dns_a_record".to_string(),
                            Value::Concrete(ConcreteValue::Bool(v)),
                        );
                    }
                    if let Some(v) = dns_opts.enable_resource_name_dns_aaaa_record() {
                        fields.insert(
                            "enable_resource_name_dns_aaaa_record".to_string(),
                            Value::Concrete(ConcreteValue::Bool(v)),
                        );
                    }
                    (!fields.is_empty()).then(|| Value::Concrete(ConcreteValue::Map(fields)))
                });
        let applied_identifier = obj.subnet_id().map(String::from);
        Ec2SubnetReadFields {
            assign_ipv6_address_on_creation,
            availability_zone,
            availability_zone_id,
            cidr_block,
            enable_dns64,
            enable_lni_at_device_index,
            ipv6_native,
            map_public_ip_on_launch,
            outpost_arn,
            private_dns_name_options_on_launch,
            subnet_id,
            vpc_id,
            applied_identifier,
        }
    }

    /// Extract ec2.InternetGateway attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_internet_gateway_attributes(
        obj: &aws_sdk_ec2::types::InternetGateway,
    ) -> Ec2InternetGatewayReadFields {
        let internet_gateway_id = obj
            .internet_gateway_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = obj.internet_gateway_id().map(String::from);
        Ec2InternetGatewayReadFields {
            internet_gateway_id,
            applied_identifier,
        }
    }

    /// Extract ec2.RouteTable attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_route_table_attributes(
        obj: &aws_sdk_ec2::types::RouteTable,
    ) -> Ec2RouteTableReadFields {
        let route_table_id = obj
            .route_table_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let vpc_id = obj
            .vpc_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = obj.route_table_id().map(String::from);
        Ec2RouteTableReadFields {
            route_table_id,
            vpc_id,
            applied_identifier,
        }
    }

    /// Extract ec2.Route attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_route_attributes(
        obj: &aws_sdk_ec2::types::Route,
    ) -> Ec2RouteReadFields {
        let destination_cidr_block = obj
            .destination_cidr_block()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let gateway_id = obj
            .gateway_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let nat_gateway_id = obj
            .nat_gateway_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = None;
        Ec2RouteReadFields {
            destination_cidr_block,
            gateway_id,
            nat_gateway_id,
            applied_identifier,
        }
    }

    /// Extract ec2.SecurityGroup attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_security_group_attributes(
        obj: &aws_sdk_ec2::types::SecurityGroup,
    ) -> Ec2SecurityGroupReadFields {
        let description = obj
            .description()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let group_id = obj
            .group_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let group_name = obj
            .group_name()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let vpc_id = obj
            .vpc_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = obj.group_id().map(String::from);
        Ec2SecurityGroupReadFields {
            description,
            group_id,
            group_name,
            vpc_id,
            applied_identifier,
        }
    }

    /// Extract ec2.SecurityGroupIngress attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_security_group_ingress_attributes(
        obj: &aws_sdk_ec2::types::SecurityGroupRule,
    ) -> Ec2SecurityGroupIngressReadFields {
        let cidr_ipv6 = obj
            .cidr_ipv6()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let description = obj
            .description()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let from_port = obj
            .from_port()
            .map(|v| Value::Concrete(ConcreteValue::Int(v as i64)));
        let group_id = obj
            .group_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let ip_protocol = obj
            .ip_protocol()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let security_group_rule_id = obj
            .security_group_rule_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let source_prefix_list_id = obj
            .prefix_list_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let to_port = obj
            .to_port()
            .map(|v| Value::Concrete(ConcreteValue::Int(v as i64)));
        let applied_identifier = obj.security_group_rule_id().map(String::from);
        Ec2SecurityGroupIngressReadFields {
            cidr_ipv6,
            description,
            from_port,
            group_id,
            ip_protocol,
            security_group_rule_id,
            source_prefix_list_id,
            to_port,
            applied_identifier,
        }
    }

    /// Extract ec2.SecurityGroupEgress attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_security_group_egress_attributes(
        obj: &aws_sdk_ec2::types::SecurityGroupRule,
    ) -> Ec2SecurityGroupEgressReadFields {
        let cidr_ipv6 = obj
            .cidr_ipv6()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let description = obj
            .description()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let destination_prefix_list_id = obj
            .prefix_list_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let from_port = obj
            .from_port()
            .map(|v| Value::Concrete(ConcreteValue::Int(v as i64)));
        let group_id = obj
            .group_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let ip_protocol = obj
            .ip_protocol()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let security_group_rule_id = obj
            .security_group_rule_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let to_port = obj
            .to_port()
            .map(|v| Value::Concrete(ConcreteValue::Int(v as i64)));
        let applied_identifier = obj.security_group_rule_id().map(String::from);
        Ec2SecurityGroupEgressReadFields {
            cidr_ipv6,
            description,
            destination_prefix_list_id,
            from_port,
            group_id,
            ip_protocol,
            security_group_rule_id,
            to_port,
            applied_identifier,
        }
    }

    /// Extract ec2.EgressOnlyInternetGateway attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_egress_only_internet_gateway_attributes(
        obj: &aws_sdk_ec2::types::EgressOnlyInternetGateway,
    ) -> Ec2EgressOnlyInternetGatewayReadFields {
        let egress_only_internet_gateway_id = obj
            .egress_only_internet_gateway_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let vpc_id = obj
            .attachments()
            .first()
            .and_then(|item| item.vpc_id())
            .map(|value| Value::Concrete(ConcreteValue::String(value.to_string().into())));
        let applied_identifier = obj.egress_only_internet_gateway_id().map(String::from);
        Ec2EgressOnlyInternetGatewayReadFields {
            egress_only_internet_gateway_id,
            vpc_id,
            applied_identifier,
        }
    }

    /// Extract ec2.Eip attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_eip_attributes(
        obj: &aws_sdk_ec2::types::Address,
    ) -> Ec2EipReadFields {
        let allocation_id = obj
            .allocation_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let domain = obj
            .domain()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let public_ip = obj
            .public_ip()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let public_ipv4_pool = obj
            .public_ipv4_pool()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = obj.allocation_id().map(String::from);
        Ec2EipReadFields {
            allocation_id,
            domain,
            public_ip,
            public_ipv4_pool,
            applied_identifier,
        }
    }

    /// Extract ec2.FlowLog attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_flow_log_attributes(
        obj: &aws_sdk_ec2::types::FlowLog,
    ) -> Ec2FlowLogReadFields {
        let deliver_logs_permission_arn = obj
            .deliver_logs_permission_arn()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let flow_log_id = obj
            .flow_log_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let log_destination = obj
            .log_destination()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let log_destination_type = obj
            .log_destination_type()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let log_format = obj
            .log_format()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let log_group_name = obj
            .log_group_name()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let max_aggregation_interval = obj
            .max_aggregation_interval()
            .map(|v| Value::Concrete(ConcreteValue::Int(v as i64)));
        let traffic_type = obj
            .traffic_type()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let applied_identifier = obj.flow_log_id().map(String::from);
        Ec2FlowLogReadFields {
            deliver_logs_permission_arn,
            flow_log_id,
            log_destination,
            log_destination_type,
            log_format,
            log_group_name,
            max_aggregation_interval,
            traffic_type,
            applied_identifier,
        }
    }

    /// Extract ec2.NatGateway attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_nat_gateway_attributes(
        obj: &aws_sdk_ec2::types::NatGateway,
    ) -> Ec2NatGatewayReadFields {
        let availability_mode = obj
            .availability_mode()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let connectivity_type = obj
            .connectivity_type()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let nat_gateway_id = obj
            .nat_gateway_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let subnet_id = obj
            .subnet_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let vpc_id = obj
            .vpc_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let allocation_id = obj
            .nat_gateway_addresses()
            .first()
            .and_then(|item| item.allocation_id())
            .map(|value| Value::Concrete(ConcreteValue::String(value.to_string().into())));
        let applied_identifier = obj.nat_gateway_id().map(String::from);
        Ec2NatGatewayReadFields {
            allocation_id,
            availability_mode,
            connectivity_type,
            nat_gateway_id,
            subnet_id,
            vpc_id,
            applied_identifier,
        }
    }

    /// Extract ec2.SubnetRouteTableAssociation attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_subnet_route_table_association_attributes(
        obj: &aws_sdk_ec2::types::RouteTableAssociation,
    ) -> Ec2SubnetRouteTableAssociationReadFields {
        let public_ipv4_pool = obj
            .public_ipv4_pool()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let route_table_id = obj
            .route_table_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let subnet_id = obj
            .subnet_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = None;
        Ec2SubnetRouteTableAssociationReadFields {
            public_ipv4_pool,
            route_table_id,
            subnet_id,
            applied_identifier,
        }
    }

    /// Extract ec2.TransitGateway attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_transit_gateway_attributes(
        obj: &aws_sdk_ec2::types::TransitGateway,
    ) -> Ec2TransitGatewayReadFields {
        let description = obj
            .description()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let transit_gateway_id = obj
            .transit_gateway_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let options = obj.options().and_then(|dns_opts| {
            let mut fields = IndexMap::new();
            if let Some(v) = dns_opts.amazon_side_asn() {
                fields.insert(
                    "amazon_side_asn".to_string(),
                    Value::Concrete(ConcreteValue::Int(v)),
                );
            }
            if let Some(v) = dns_opts.auto_accept_shared_attachments() {
                fields.insert(
                    "auto_accept_shared_attachments".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            if let Some(v) = dns_opts.default_route_table_association() {
                fields.insert(
                    "default_route_table_association".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            if let Some(v) = dns_opts.default_route_table_propagation() {
                fields.insert(
                    "default_route_table_propagation".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            if let Some(v) = dns_opts.dns_support() {
                fields.insert(
                    "dns_support".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            if let Some(v) = dns_opts.multicast_support() {
                fields.insert(
                    "multicast_support".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            if let Some(v) = dns_opts.security_group_referencing_support() {
                fields.insert(
                    "security_group_referencing_support".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            {
                let v = dns_opts.transit_gateway_cidr_blocks();
                if !v.is_empty() {
                    fields.insert(
                        "transit_gateway_cidr_blocks".to_string(),
                        Value::Concrete(ConcreteValue::List(
                            v.iter()
                                .map(|s| {
                                    Value::Concrete(ConcreteValue::String(s.to_string().into()))
                                })
                                .collect(),
                        )),
                    );
                }
            }
            if let Some(v) = dns_opts.vpn_ecmp_support() {
                fields.insert(
                    "vpn_ecmp_support".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            (!fields.is_empty()).then(|| Value::Concrete(ConcreteValue::Map(fields)))
        });
        let applied_identifier = obj.transit_gateway_id().map(String::from);
        Ec2TransitGatewayReadFields {
            description,
            options,
            transit_gateway_id,
            applied_identifier,
        }
    }

    /// Extract ec2.TransitGatewayAttachment attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_transit_gateway_attachment_attributes(
        obj: &aws_sdk_ec2::types::TransitGatewayVpcAttachment,
    ) -> Ec2TransitGatewayAttachmentReadFields {
        let ids = obj.subnet_ids();
        let subnet_ids = (!ids.is_empty()).then(|| {
            let list: Vec<Value> = ids
                .iter()
                .map(|s| Value::Concrete(ConcreteValue::String(s.to_string().into())))
                .collect();
            Value::Concrete(ConcreteValue::List(list))
        });
        let transit_gateway_attachment_id = obj
            .transit_gateway_attachment_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let transit_gateway_id = obj
            .transit_gateway_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let vpc_id = obj
            .vpc_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let options = obj.options().and_then(|dns_opts| {
            let mut fields = IndexMap::new();
            if let Some(v) = dns_opts.appliance_mode_support() {
                fields.insert(
                    "appliance_mode_support".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            if let Some(v) = dns_opts.dns_support() {
                fields.insert(
                    "dns_support".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            if let Some(v) = dns_opts.ipv6_support() {
                fields.insert(
                    "ipv6_support".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            if let Some(v) = dns_opts.security_group_referencing_support() {
                fields.insert(
                    "security_group_referencing_support".to_string(),
                    Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())),
                );
            }
            (!fields.is_empty()).then(|| Value::Concrete(ConcreteValue::Map(fields)))
        });
        let applied_identifier = obj.transit_gateway_attachment_id().map(String::from);
        Ec2TransitGatewayAttachmentReadFields {
            options,
            subnet_ids,
            transit_gateway_attachment_id,
            transit_gateway_id,
            vpc_id,
            applied_identifier,
        }
    }

    /// Extract ec2.VpcEndpoint attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_vpc_endpoint_attributes(
        obj: &aws_sdk_ec2::types::VpcEndpoint,
    ) -> Ec2VpcEndpointReadFields {
        let policy_document = obj
            .policy_document()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let private_dns_enabled = obj
            .private_dns_enabled()
            .map(|v| Value::Concrete(ConcreteValue::Bool(v)));
        let resource_configuration_arn = obj
            .resource_configuration_arn()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let ids = obj.route_table_ids();
        let route_table_ids = (!ids.is_empty()).then(|| {
            let list: Vec<Value> = ids
                .iter()
                .map(|s| Value::Concrete(ConcreteValue::String(s.to_string().into())))
                .collect();
            Value::Concrete(ConcreteValue::List(list))
        });
        let service_name = obj
            .service_name()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let service_network_arn = obj
            .service_network_arn()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let service_region = obj
            .service_region()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let ids = obj.subnet_ids();
        let subnet_ids = (!ids.is_empty()).then(|| {
            let list: Vec<Value> = ids
                .iter()
                .map(|s| Value::Concrete(ConcreteValue::String(s.to_string().into())))
                .collect();
            Value::Concrete(ConcreteValue::List(list))
        });
        let vpc_endpoint_id = obj
            .vpc_endpoint_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let vpc_endpoint_type = obj
            .vpc_endpoint_type()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let vpc_id = obj
            .vpc_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let list: Vec<Value> = obj
            .groups()
            .iter()
            .filter_map(|item| {
                item.group_id()
                    .map(|id| Value::Concrete(ConcreteValue::String(id.to_string().into())))
            })
            .collect();
        let security_group_ids =
            (!list.is_empty()).then(|| Value::Concrete(ConcreteValue::List(list)));
        let applied_identifier = obj.vpc_endpoint_id().map(String::from);
        Ec2VpcEndpointReadFields {
            policy_document,
            private_dns_enabled,
            resource_configuration_arn,
            route_table_ids,
            security_group_ids,
            service_name,
            service_network_arn,
            service_region,
            subnet_ids,
            vpc_endpoint_id,
            vpc_endpoint_type,
            vpc_id,
            applied_identifier,
        }
    }

    /// Extract ec2.VpcPeeringConnection attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_vpc_peering_connection_attributes(
        obj: &aws_sdk_ec2::types::VpcPeeringConnection,
    ) -> Ec2VpcPeeringConnectionReadFields {
        let vpc_peering_connection_id = obj
            .vpc_peering_connection_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let vpc_id = obj
            .requester_vpc_info()
            .and_then(|opts| opts.vpc_id())
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let peer_vpc_id = obj
            .accepter_vpc_info()
            .and_then(|opts| opts.vpc_id())
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let peer_owner_id = obj
            .accepter_vpc_info()
            .and_then(|opts| opts.owner_id())
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = obj.vpc_peering_connection_id().map(String::from);
        Ec2VpcPeeringConnectionReadFields {
            peer_owner_id,
            peer_vpc_id,
            vpc_id,
            vpc_peering_connection_id,
            applied_identifier,
        }
    }

    /// Extract ec2.VpnGateway attributes from SDK response type (generated)
    pub(crate) fn extract_ec2_vpn_gateway_attributes(
        obj: &aws_sdk_ec2::types::VpnGateway,
    ) -> Ec2VpnGatewayReadFields {
        let amazon_side_asn = obj
            .amazon_side_asn()
            .map(|v| Value::Concrete(ConcreteValue::Int(v)));
        let availability_zone = obj
            .availability_zone()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let r#type = obj
            .r#type()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let vpn_gateway_id = obj
            .vpn_gateway_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = obj.vpn_gateway_id().map(String::from);
        Ec2VpnGatewayReadFields {
            amazon_side_asn,
            availability_zone,
            r#type,
            vpn_gateway_id,
            applied_identifier,
        }
    }

    /// Extract organizations.Organization attributes from SDK response type (generated)
    pub(crate) fn extract_organizations_organization_attributes(
        obj: &aws_sdk_organizations::types::Organization,
    ) -> OrganizationsOrganizationReadFields {
        let arn = obj
            .arn()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let feature_set = obj
            .feature_set()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let id = obj
            .id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let master_account_arn = obj
            .master_account_arn()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let master_account_email = obj
            .master_account_email()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let master_account_id = obj
            .master_account_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = obj.id().map(String::from);
        OrganizationsOrganizationReadFields {
            arn,
            feature_set,
            id,
            master_account_arn,
            master_account_email,
            master_account_id,
            applied_identifier,
        }
    }

    /// Extract organizations.Account attributes from SDK response type (generated)
    pub(crate) fn extract_organizations_account_attributes(
        obj: &aws_sdk_organizations::types::Account,
    ) -> OrganizationsAccountReadFields {
        let arn = obj
            .arn()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let email = obj
            .email()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let id = obj
            .id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let joined_method = obj
            .joined_method()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let name = obj
            .name()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let status = obj
            .status()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let applied_identifier = obj.id().map(String::from);
        OrganizationsAccountReadFields {
            arn,
            email,
            id,
            joined_method,
            name,
            status,
            applied_identifier,
        }
    }

    /// Extract route53.RecordSet attributes from SDK response type (generated)
    pub(crate) fn extract_route53_record_set_attributes(
        obj: &aws_sdk_route53::types::ResourceRecordSet,
    ) -> Route53RecordSetReadFields {
        let v = obj.name();
        let name =
            (!v.is_empty()).then(|| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let ttl = obj.ttl().map(|v| Value::Concrete(ConcreteValue::Int(v)));
        let r#type = Some(Value::Concrete(ConcreteValue::String(
            obj.r#type().as_str().to_string().into(),
        )));
        let applied_identifier = Some(obj.name().to_string());
        Route53RecordSetReadFields {
            name,
            ttl,
            r#type,
            applied_identifier,
        }
    }

    /// Extract iam.Role attributes from SDK response type (generated)
    pub(crate) fn extract_iam_role_attributes(obj: &aws_sdk_iam::types::Role) -> IamRoleReadFields {
        let v = obj.arn();
        let arn =
            (!v.is_empty()).then(|| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let assume_role_policy_document = obj
            .assume_role_policy_document()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let description = obj
            .description()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let max_session_duration = obj
            .max_session_duration()
            .map(|v| Value::Concrete(ConcreteValue::Int(v as i64)));
        let v = obj.path();
        let path =
            (!v.is_empty()).then(|| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let v = obj.role_id();
        let role_id =
            (!v.is_empty()).then(|| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let v = obj.role_name();
        let role_name =
            (!v.is_empty()).then(|| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let applied_identifier = Some(obj.role_name().to_string());
        IamRoleReadFields {
            arn,
            assume_role_policy_document,
            description,
            max_session_duration,
            path,
            role_id,
            role_name,
            applied_identifier,
        }
    }

    /// Extract logs.LogGroup attributes from SDK response type (generated)
    pub(crate) fn extract_logs_log_group_attributes(
        obj: &aws_sdk_cloudwatchlogs::types::LogGroup,
    ) -> LogsLogGroupReadFields {
        let deletion_protection_enabled = obj
            .deletion_protection_enabled()
            .map(|v| Value::Concrete(ConcreteValue::Bool(v)));
        let kms_key_id = obj
            .kms_key_id()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let log_group_class = obj
            .log_group_class()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let log_group_name = obj
            .log_group_name()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let retention_in_days = obj
            .retention_in_days()
            .map(|v| Value::Concrete(ConcreteValue::Int(v as i64)));
        let applied_identifier = None;
        LogsLogGroupReadFields {
            deletion_protection_enabled,
            kms_key_id,
            log_group_class,
            log_group_name,
            retention_in_days,
            applied_identifier,
        }
    }

    /// Extract acm.Certificate attributes from SDK response type (generated)
    pub(crate) fn extract_acm_certificate_attributes(
        obj: &aws_sdk_acm::types::CertificateDetail,
    ) -> AcmCertificateReadFields {
        let certificate_arn = obj
            .certificate_arn()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let domain_name = obj
            .domain_name()
            .map(|v| Value::Concrete(ConcreteValue::String(v.to_string().into())));
        let key_algorithm = obj
            .key_algorithm()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let renewal_eligibility = obj
            .renewal_eligibility()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let status = obj
            .status()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let ids = obj.subject_alternative_names();
        let subject_alternative_names = (!ids.is_empty()).then(|| {
            let list: Vec<Value> = ids
                .iter()
                .map(|s| Value::Concrete(ConcreteValue::String(s.to_string().into())))
                .collect();
            Value::Concrete(ConcreteValue::List(list))
        });
        let r#type = obj
            .r#type()
            .map(|v| Value::Concrete(ConcreteValue::String(v.as_str().to_string().into())));
        let applied_identifier = obj.certificate_arn().map(String::from);
        AcmCertificateReadFields {
            certificate_arn,
            domain_name,
            key_algorithm,
            renewal_eligibility,
            status,
            subject_alternative_names,
            r#type,
            applied_identifier,
        }
    }
}

// ===== Generated DataSourceLookups Trait =====

/// One method per `DataSourceDef`. AwsProvider must implement all of
/// them; the codegen-emitted dispatcher below routes by
/// `resource.id.resource_type`.
pub trait DataSourceLookups {
    fn read_sts_caller_identity_data_source(
        &self,
        resource: &DataSource<impl Clone + Send + Sync + 'static>,
    ) -> BoxFuture<'_, ProviderResult<State>>;

    fn read_identitystore_user_data_source(
        &self,
        resource: &DataSource<impl Clone + Send + Sync + 'static>,
    ) -> BoxFuture<'_, ProviderResult<State>>;

    fn read_s3_bucket_data_source(
        &self,
        resource: &DataSource<impl Clone + Send + Sync + 'static>,
    ) -> BoxFuture<'_, ProviderResult<State>>;

    fn read_iam_roles_data_source(
        &self,
        resource: &DataSource<impl Clone + Send + Sync + 'static>,
    ) -> BoxFuture<'_, ProviderResult<State>>;
}

// ===== Generated read_data_source dispatcher =====

/// Routes `Provider::read_data_source` calls to the matching
/// `DataSourceLookups` trait method. The default arm refuses
/// to drop user-supplied inputs silently.
pub(crate) fn dispatch_read_data_source<'a>(
    provider: &'a AwsProvider,
    resource: &'a DataSource<impl Clone + Send + Sync + 'static>,
) -> BoxFuture<'a, ProviderResult<State>> {
    match resource.id.resource_type.as_str() {
        "sts.CallerIdentity" => provider.read_sts_caller_identity_data_source(resource),
        "identitystore.User" => provider.read_identitystore_user_data_source(resource),
        "s3.Bucket" => provider.read_s3_bucket_data_source(resource),
        "iam.Roles" => provider.read_iam_roles_data_source(resource),
        _ => {
            let id = resource.id.clone();
            let resource_type = resource.id.resource_type.clone();
            Box::pin(async move {
                Err(ProviderError::internal(format!(
                    "aws provider does not implement read_data_source for '{}'",
                    resource_type
                ))
                .for_resource(id))
            })
        }
    }
}
