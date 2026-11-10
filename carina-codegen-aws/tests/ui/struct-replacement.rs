use carina_codegen_aws::resource_defs::route53_resources;
fn main() {
    route53_resources()[0].struct_field_type_overrides.push(("AliasTarget", "HostedZoneId", "AttributeType::string()"));
}
