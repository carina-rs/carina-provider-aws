use carina_codegen_aws::resource_defs::iam_data_sources;
fn main() {
    iam_data_sources()[0].inputs[0].type_override = Some("AttributeType::string()");
}
