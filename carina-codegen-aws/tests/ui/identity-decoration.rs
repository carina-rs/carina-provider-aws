use carina_codegen_aws::{overrides::TypeOverride, resource_defs::ec2_resources};
fn main() {
    ec2_resources()[0].type_overrides.push(("Label", TypeOverride::StringBounds { min: 1, max: 10 }));
}
