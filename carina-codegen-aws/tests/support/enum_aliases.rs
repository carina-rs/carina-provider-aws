fn aliases(
    ty: &AttributeType,
    path: &str,
    out: &mut std::collections::BTreeMap<String, Vec<(String, String)>>,
) {
    match ty.raw_shape() {
        RawShape::Enum { dsl_aliases, .. } => {
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
