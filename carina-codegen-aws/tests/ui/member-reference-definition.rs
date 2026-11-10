fn main() {
    let named = carina_aws_types::kms_key_reference();
    carina_aws_types::with_member_reference(carina_core::schema::AttributeType::string(), named);
}
