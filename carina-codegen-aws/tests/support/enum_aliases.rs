fn aliases(
    ty: &AttributeType,
    path: &str,
    out: &mut std::collections::BTreeMap<String, Vec<(String, String)>>,
) {
    match ty.raw_shape() {
        RawShape::Enum { identity, dsl_aliases, .. } => {
            if let Some(row) = carina_aws_types::AwsType::ALL.iter().find(|row| &row.identity() == identity) {
                let definition = row.definition();
                // Exercise both identity-attachment carriers in the docs walk
                // and in the compiled emitted-schema walk. The catalog owns
                // the presentation in both cases.
                let RawShape::Enum {dsl_aliases: expected,..} = definition.as_attribute_type().raw_shape() else {panic!("Enum row")};
                for source in [AttributeType::string(), definition.clone().into_attribute_type()] {
                    let attached = carina_aws_types::try_with_member_identity(source, definition.clone()).unwrap();
                    let RawShape::Enum {dsl_aliases: actual,..} = attached.raw_shape() else {panic!("Enum attachment")};
                    assert_eq!(actual,expected,"{path}: catalog aliases must own both attachment branches");
                }
            }
            out.insert(path.into(), dsl_aliases.to_vec());
        }
        RawShape::List { element_type, .. } => aliases(element_type, &format!("{path}[]"), out),
        RawShape::Map { key, value, .. } => {
            aliases(key, &format!("{path}{{key}}"), out);
            aliases(value, &format!("{path}{{}}"), out);
        }
        RawShape::Struct { fields, .. } => {
            for field in fields {
                aliases(
                    field.parts().field_type,
                    &format!("{path}.{}", field.name()),
                    out,
                )
            }
        }
        RawShape::Union(members) => {
            for (i, member) in members.iter().enumerate() {
                aliases(member, &format!("{path}@{i}"), out)
            }
        }
        _ => {}
    }
}
