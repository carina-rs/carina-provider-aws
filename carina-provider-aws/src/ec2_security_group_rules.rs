//! EC2 security group rule shared CRUD operations

use carina_core::provider::{AppliedIdentifier, ReadOutcome};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, ResourceId, Value};
use carina_core::schema::ResourceSchema;

use crate::AwsProvider;
use crate::error_helpers::api_error_with_meta;
use crate::helpers::require_enum_attr;
use crate::provider::CreateCompletion;
use crate::provider_generated::{
    Ec2SecurityGroupEgressReadFields, Ec2SecurityGroupIngressReadFields,
};
use crate::read_model::{found_managed_read, require_read_value};
use crate::read_models_generated::{
    Ec2SecurityGroupEgressReadModel, Ec2SecurityGroupIngressReadModel,
};

fn build_security_group_rule_permission(
    resource: &Resource<impl Sized>,
    schema: &ResourceSchema,
    is_ingress: bool,
) -> ProviderResult<aws_sdk_ec2::types::IpPermission> {
    let protocol = require_enum_attr(resource, schema, "ip_protocol")?;

    let from_port = match resource.get_attr("from_port") {
        Some(Value::Concrete(ConcreteValue::Int(n))) => *n as i32,
        _ => 0,
    };

    let to_port = match resource.get_attr("to_port") {
        Some(Value::Concrete(ConcreteValue::Int(n))) => *n as i32,
        _ => 0,
    };

    let cidr_ip = match resource.get_attr("cidr_ip") {
        Some(Value::Concrete(ConcreteValue::String(s))) => Some(s.expose_plaintext().to_string()),
        _ => None,
    };

    let cidr_ipv6 = match resource.get_attr("cidr_ipv6") {
        Some(Value::Concrete(ConcreteValue::String(s))) => Some(s.expose_plaintext().to_string()),
        _ => None,
    };

    let description = match resource.get_attr("description") {
        Some(Value::Concrete(ConcreteValue::String(s))) => Some(s.expose_plaintext().to_string()),
        _ => None,
    };

    let prefix_list_attr = if is_ingress {
        "source_prefix_list_id"
    } else {
        "destination_prefix_list_id"
    };
    let prefix_list_id = match resource.get_attr(prefix_list_attr) {
        Some(Value::Concrete(ConcreteValue::String(s))) => Some(s.expose_plaintext().to_string()),
        _ => None,
    };

    let sg_ref_attr = if is_ingress {
        "source_security_group_id"
    } else {
        "destination_security_group_id"
    };
    let ref_security_group_id = match resource.get_attr(sg_ref_attr) {
        Some(Value::Concrete(ConcreteValue::String(s))) => Some(s.expose_plaintext().to_string()),
        _ => None,
    };

    let mut permission_builder = aws_sdk_ec2::types::IpPermission::builder()
        .ip_protocol(&protocol)
        .from_port(from_port)
        .to_port(to_port);

    if let Some(ref cidr) = cidr_ip {
        let mut range_builder = aws_sdk_ec2::types::IpRange::builder().cidr_ip(cidr);
        if let Some(ref desc) = description {
            range_builder = range_builder.description(desc);
        }
        permission_builder = permission_builder.ip_ranges(range_builder.build());
    }

    if let Some(ref cidr_v6) = cidr_ipv6 {
        let mut range_builder = aws_sdk_ec2::types::Ipv6Range::builder().cidr_ipv6(cidr_v6);
        if let Some(ref desc) = description {
            range_builder = range_builder.description(desc);
        }
        permission_builder = permission_builder.ipv6_ranges(range_builder.build());
    }

    if let Some(ref pl_id) = prefix_list_id {
        let mut pl_builder = aws_sdk_ec2::types::PrefixListId::builder().prefix_list_id(pl_id);
        if let Some(ref desc) = description {
            pl_builder = pl_builder.description(desc);
        }
        permission_builder = permission_builder.prefix_list_ids(pl_builder.build());
    }

    if let Some(ref ref_sg_id) = ref_security_group_id {
        let mut pair_builder = aws_sdk_ec2::types::UserIdGroupPair::builder().group_id(ref_sg_id);
        if let Some(Value::Concrete(ConcreteValue::String(owner_id))) =
            resource.get_attr("source_security_group_owner_id")
        {
            pair_builder = pair_builder.user_id(owner_id.expose_plaintext());
        }
        if let Some(ref desc) = description {
            pair_builder = pair_builder.description(desc);
        }
        permission_builder = permission_builder.user_id_group_pairs(pair_builder.build());
    }

    Ok(permission_builder.build())
}

fn require_security_group_rule_group_id(resource: &Resource<impl Sized>) -> ProviderResult<String> {
    match resource.get_attr("group_id") {
        Some(Value::Concrete(ConcreteValue::String(s))) => Ok(s.expose_plaintext().to_string()),
        _ => Err(
            ProviderError::invalid_input("Security Group ID (group_id) is required")
                .for_resource(resource.id.clone()),
        ),
    }
}

impl AwsProvider {
    /// Read an EC2 Security Group Rule (shared between ingress and egress)
    pub(crate) async fn read_ec2_security_group_rule(
        &self,
        id: &ResourceId,
        identifier: &AppliedIdentifier,
        is_ingress: bool,
    ) -> ProviderResult<ReadOutcome> {
        let applied_identifier = identifier;
        let identifier = identifier.as_str();

        // Look up by rule IDs (may be comma-separated)
        let rule_ids: Vec<&str> = identifier.split(',').collect();
        let mut req = self.ec2_client.describe_security_group_rules();
        for rule_id in &rule_ids {
            req = req.security_group_rule_ids(*rule_id);
        }
        let result = req.send().await.map_err(|e| {
            api_error_with_meta("Failed to describe security group rules", e)
                .for_resource(id.clone())
        })?;
        let rules: Vec<_> = result
            .security_group_rules()
            .iter()
            .filter(|rule| rule.is_egress() == Some(!is_ingress))
            .cloned()
            .collect();

        if rules.is_empty() {
            return Ok(ReadOutcome::not_found(id.clone()));
        }

        // Use the first rule for common attributes.
        let first_rule = &rules[0];

        // Override rule IDs with comma-separated values (multi-rule support).
        let rule_ids: Vec<String> = rules
            .iter()
            .filter_map(|r| r.security_group_rule_id().map(String::from))
            .collect();
        let combined_rule_id = (!rule_ids.is_empty())
            .then(|| Value::Concrete(ConcreteValue::String(rule_ids.join(",").into())));

        // IPv4 CIDR (CidrIp in schema maps to CidrIpv4 in SDK).
        let cidr_ip = first_rule
            .cidr_ipv4()
            .map(|cidr| Value::Concrete(ConcreteValue::String(cidr.to_string().into())));

        // Referenced security-group fields live in a nested SDK structure.
        let referenced_group_id = first_rule
            .referenced_group_info()
            .and_then(|group| group.group_id())
            .map(|group_id| Value::Concrete(ConcreteValue::String(group_id.to_string().into())));
        let source_security_group_owner_id = first_rule
            .referenced_group_info()
            .and_then(|group| group.user_id())
            .map(|owner_id| Value::Concrete(ConcreteValue::String(owner_id.to_string().into())));

        if is_ingress {
            let Ec2SecurityGroupIngressReadFields {
                cidr_ipv6,
                description,
                from_port,
                group_id,
                ip_protocol,
                security_group_rule_id,
                source_prefix_list_id,
                to_port,
                applied_identifier: _,
            } = Self::extract_ec2_security_group_ingress_attributes(first_rule);
            let model = Ec2SecurityGroupIngressReadModel {
                cidr_ip,
                cidr_ipv6,
                description,
                from_port,
                group_id,
                ip_protocol: require_read_value(id, ip_protocol, "ip_protocol")?,
                source_prefix_list_id,
                source_security_group_owner_id,
                to_port,
                source_security_group_id: referenced_group_id,
                security_group_rule_id: combined_rule_id.or(security_group_rule_id),
            };
            found_managed_read(id, applied_identifier, model)
        } else {
            let Ec2SecurityGroupEgressReadFields {
                cidr_ipv6,
                description,
                destination_prefix_list_id,
                from_port,
                group_id,
                ip_protocol,
                security_group_rule_id,
                to_port,
                applied_identifier: _,
            } = Self::extract_ec2_security_group_egress_attributes(first_rule);
            let model = Ec2SecurityGroupEgressReadModel {
                cidr_ip,
                cidr_ipv6,
                description,
                destination_prefix_list_id,
                from_port,
                group_id: require_read_value(id, group_id, "group_id")?,
                ip_protocol: require_read_value(id, ip_protocol, "ip_protocol")?,
                source_security_group_owner_id,
                to_port,
                destination_security_group_id: referenced_group_id,
                security_group_rule_id: combined_rule_id.or(security_group_rule_id),
            };
            found_managed_read(id, applied_identifier, model)
        }
    }

    /// Create an EC2 Security Group Rule (shared between ingress and egress)
    pub(crate) async fn create_ec2_security_group_rule(
        &self,
        resource: &Resource<impl Sized>,
        schema: &ResourceSchema,
        is_ingress: bool,
    ) -> ProviderResult<CreateCompletion> {
        let sg_id = require_security_group_rule_group_id(resource)?;
        let permission = build_security_group_rule_permission(resource, schema, is_ingress)?;
        let (identifier, outcome) = self
            .create_ec2_security_group_rule_with_permission(resource, sg_id, permission, is_ingress)
            .await?;
        Ok(self.create_outcome_from_read(
            &resource.id,
            identifier,
            resource.resolved_attributes(),
            outcome,
        ))
    }

    async fn create_ec2_security_group_rule_with_permission(
        &self,
        resource: &Resource<impl Sized>,
        sg_id: String,
        permission: aws_sdk_ec2::types::IpPermission,
        is_ingress: bool,
    ) -> ProviderResult<(AppliedIdentifier, ProviderResult<ReadOutcome>)> {
        let rule_ids: Vec<String> = if is_ingress {
            let result = self
                .ec2_client
                .authorize_security_group_ingress()
                .group_id(&sg_id)
                .ip_permissions(permission)
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("Failed to create ingress rule", e)
                        .for_resource(resource.id.clone())
                })?;

            result
                .security_group_rules()
                .iter()
                .filter_map(|r| r.security_group_rule_id().map(String::from))
                .collect()
        } else {
            let result = self
                .ec2_client
                .authorize_security_group_egress()
                .group_id(&sg_id)
                .ip_permissions(permission)
                .send()
                .await
                .map_err(|e| {
                    api_error_with_meta("Failed to create egress rule", e)
                        .for_resource(resource.id.clone())
                })?;

            result
                .security_group_rules()
                .iter()
                .filter_map(|r| r.security_group_rule_id().map(String::from))
                .collect()
        };

        // Read back using rule IDs (reliable identifier)
        let identifier = rule_ids.join(",");
        let operation = if is_ingress {
            "ec2.AuthorizeSecurityGroupIngress"
        } else {
            "ec2.AuthorizeSecurityGroupEgress"
        };
        for rule_id in &rule_ids {
            crate::helpers::applied_identifier(&resource.id, operation, rule_id)?;
        }
        let identifier = crate::helpers::applied_identifier(&resource.id, operation, identifier)?;
        let outcome = self
            .read_ec2_security_group_rule(&resource.id, &identifier, is_ingress)
            .await;
        Ok((identifier, outcome))
    }

    /// Update an EC2 Security Group Rule (rules are immutable, so recreate)
    pub(crate) async fn update_ec2_security_group_rule(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        to: Resource,
        schema: &ResourceSchema,
        is_ingress: bool,
    ) -> ProviderResult<ProviderResult<ReadOutcome>> {
        // Validate and build every fallible local input before deleting the
        // existing immutable rule. An invalid replacement must leave it intact.
        let sg_id = require_security_group_rule_group_id(&to)?;
        let permission = build_security_group_rule_permission(&to, schema, is_ingress)?;

        self.delete_ec2_security_group_rule(id.clone(), identifier, is_ingress)
            .await?;
        let (_, outcome) = self
            .create_ec2_security_group_rule_with_permission(&to, sg_id, permission, is_ingress)
            .await?;
        Ok(outcome)
    }

    /// Delete an EC2 Security Group Rule (deletes all rules by identifier)
    pub(crate) async fn delete_ec2_security_group_rule(
        &self,
        id: ResourceId,
        identifier: &AppliedIdentifier,
        is_ingress: bool,
    ) -> ProviderResult<()> {
        let identifier = identifier.as_str();
        // identifier is comma-separated rule IDs (e.g., "sgr-123,sgr-456")
        let rule_ids: Vec<&str> = identifier.split(',').collect();

        // Look up the rules to get the security group ID
        let mut req = self.ec2_client.describe_security_group_rules();
        for rule_id in &rule_ids {
            req = req.security_group_rule_ids(*rule_id);
        }
        let result = req.send().await.map_err(|e| {
            api_error_with_meta("Failed to describe security group rules", e)
                .for_resource(id.clone())
        })?;

        let rules = result.security_group_rules();
        if rules.is_empty() {
            return Err(
                ProviderError::not_found("Security Group Rule not found").for_resource(id.clone())
            );
        }

        let sg_id = rules[0].group_id().ok_or_else(|| {
            ProviderError::internal("Rule has no security group ID").for_resource(id.clone())
        })?;

        // Delete all rules at once
        if is_ingress {
            let mut request = self
                .ec2_client
                .revoke_security_group_ingress()
                .group_id(sg_id);
            for rule_id in &rule_ids {
                request = request.security_group_rule_ids(*rule_id);
            }
            request.send().await.map_err(|e| {
                api_error_with_meta("Failed to delete ingress rules", e).for_resource(id.clone())
            })?;
        } else {
            let mut request = self
                .ec2_client
                .revoke_security_group_egress()
                .group_id(sg_id);
            for rule_id in &rule_ids {
                request = request.security_group_rule_ids(*rule_id);
            }
            request.send().await.map_err(|e| {
                api_error_with_meta("Failed to delete egress rules", e).for_resource(id.clone())
            })?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_ip_protocol_is_an_error_not_an_all_protocols_default() {
        let schema = crate::schemas::generated::ec2::security_group_ingress::ec2_security_group_ingress_config().schema;
        let mut resource = Resource::with_provider("aws", "ec2.SecurityGroupIngress", "test", None);
        resource.set_attr(
            "ip_protocol".to_string(),
            Value::Concrete(ConcreteValue::String("not-a-protocol".into())),
        );

        let error = build_security_group_rule_permission(&resource, &schema, true)
            .expect_err("invalid ip_protocol must not fall back to -1");
        assert!(
            error
                .to_string()
                .contains("does not exactly match a declared API value")
        );
    }

    // --- Route composite identifier parsing tests ---

    #[test]
    fn test_route_identifier_parsing() {
        let identifier = "rtb-12345678|0.0.0.0/0";
        let (route_table_id, destination) = identifier.split_once('|').unwrap();
        assert_eq!(route_table_id, "rtb-12345678");
        assert_eq!(destination, "0.0.0.0/0");
    }

    #[test]
    fn test_route_identifier_parsing_no_separator() {
        let identifier = "rtb-12345678";
        assert_eq!(identifier.split_once('|'), None);
    }

    #[test]
    fn test_route_identifier_parsing_ipv6_destination() {
        let identifier = "rtb-12345678|::/0";
        let (route_table_id, destination) = identifier.split_once('|').unwrap();
        assert_eq!(route_table_id, "rtb-12345678");
        assert_eq!(destination, "::/0");
    }

    // --- Security group rule referenced group extraction ---

    #[test]
    fn test_security_group_rule_referenced_group() {
        let ref_group = aws_sdk_ec2::types::ReferencedSecurityGroup::builder()
            .group_id("sg-ref-12345678")
            .build();
        let rule = aws_sdk_ec2::types::SecurityGroupRule::builder()
            .security_group_rule_id("sgr-12345678")
            .group_id("sg-12345678")
            .ip_protocol("tcp")
            .from_port(443)
            .to_port(443)
            .referenced_group_info(ref_group)
            .build();

        // Replicate logic from read_ec2_security_group_rule for ingress
        let source_security_group_id = rule
            .referenced_group_info()
            .and_then(|group| group.group_id())
            .map(|group_id| Value::Concrete(ConcreteValue::String(group_id.to_string().into())));

        assert_eq!(
            source_security_group_id.as_ref(),
            Some(&Value::Concrete(ConcreteValue::String(
                "sg-ref-12345678".to_string().into()
            )))
        );
    }

    #[test]
    fn test_security_group_rule_cidr_ipv4() {
        let rule = aws_sdk_ec2::types::SecurityGroupRule::builder()
            .security_group_rule_id("sgr-12345678")
            .group_id("sg-12345678")
            .ip_protocol("tcp")
            .from_port(80)
            .to_port(80)
            .cidr_ipv4("10.0.0.0/8")
            .build();

        // Replicate logic from read_ec2_security_group_rule
        let cidr_ip = rule
            .cidr_ipv4()
            .map(|cidr_ip| Value::Concrete(ConcreteValue::String(cidr_ip.to_string().into())));

        assert_eq!(
            cidr_ip.as_ref(),
            Some(&Value::Concrete(ConcreteValue::String(
                "10.0.0.0/8".to_string().into()
            )))
        );
    }

    // --- Security group rule is_egress filtering ---

    #[test]
    fn test_security_group_rule_is_egress_filtering() {
        let ingress_rule = aws_sdk_ec2::types::SecurityGroupRule::builder()
            .security_group_rule_id("sgr-ingress")
            .is_egress(false)
            .build();
        let egress_rule = aws_sdk_ec2::types::SecurityGroupRule::builder()
            .security_group_rule_id("sgr-egress")
            .is_egress(true)
            .build();

        let rules = [ingress_rule, egress_rule];

        // Filter for ingress (is_ingress=true means is_egress should be false)
        let ingress_filtered: Vec<_> = rules
            .iter()
            .filter(|rule| rule.is_egress() == Some(false))
            .collect();
        assert_eq!(ingress_filtered.len(), 1);
        assert_eq!(
            ingress_filtered[0].security_group_rule_id(),
            Some("sgr-ingress")
        );

        // Filter for egress (is_ingress=false means is_egress should be true)
        let egress_filtered: Vec<_> = rules
            .iter()
            .filter(|rule| rule.is_egress() == Some(true))
            .collect();
        assert_eq!(egress_filtered.len(), 1);
        assert_eq!(
            egress_filtered[0].security_group_rule_id(),
            Some("sgr-egress")
        );
    }

    // --- Security group rule comma-separated identifiers ---

    #[test]
    fn test_security_group_rule_comma_separated_ids() {
        // Tests the comma-separated rule ID pattern used in multi-rule support
        let identifier = "sgr-111,sgr-222,sgr-333";
        let rule_ids: Vec<&str> = identifier.split(',').collect();
        assert_eq!(rule_ids.len(), 3);
        assert_eq!(rule_ids[0], "sgr-111");
        assert_eq!(rule_ids[1], "sgr-222");
        assert_eq!(rule_ids[2], "sgr-333");
    }

    #[test]
    fn test_security_group_rule_single_id() {
        let identifier = "sgr-111";
        let rule_ids: Vec<&str> = identifier.split(',').collect();
        assert_eq!(rule_ids.len(), 1);
        assert_eq!(rule_ids[0], "sgr-111");
    }
}
