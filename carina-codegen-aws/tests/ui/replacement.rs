use carina_codegen_aws::resource_defs::ec2_resources;

fn main() {
    let mut resource = ec2_resources().remove(0);
    resource.type_overrides.push(("Label", "AttributeType::bool()"));
}
