fn main() {
    let named = carina_core::schema::AttributeType::int();
    carina_aws_types::try_with_member_identity(carina_core::schema::AttributeType::int(), named).unwrap();
}
