use carina_codegen_aws::resource_defs::ec2_resources;
fn main() {
    ec2_resources()[0].type_overrides.push(("Label", || carina_core::schema::AttributeType::bool()));
}
