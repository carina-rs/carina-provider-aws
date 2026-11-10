use carina_core::schema::{AttributeType, TypeIdentity};
fn identity(_: &[&str], _: &str) -> TypeIdentity { TypeIdentity::bare("Unowned") }
carina_aws_types::define_aws_type_catalog! {
    pub enum Unowned {
        identity_constructor: identity,
        refinement_type: Unowned,
        rows: {
            PatternOnly => pattern_only {
                segments: [], kind: "String", valid_sample: "x",
                constructor: |identity| AttributeType::refined_string(Some(identity), None, Some((Some(1), Some(2))), None),
            }
        }
    }
}
fn main() {}
