fn main() {
    let named = carina_core::schema::AttributeType::string();
    carina_aws_types::with_member_identity(carina_core::schema::AttributeType::string(), named);
}
