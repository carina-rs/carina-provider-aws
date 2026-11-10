fn main() {
    let constrained = carina_core::schema::AttributeType::string_member(
        carina_core::schema::StringIdentityLayer::new(None, None, None, None),
        carina_core::schema::StringMemberConstraints::new(None, Some((Some(1), Some(4)))),
    );
    let _ = carina_aws_types::with_named_enum_values(constrained, &["A"], &[]);
}
