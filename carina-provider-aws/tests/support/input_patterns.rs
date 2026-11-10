//! Evaluate the additional portable pattern declarations for one authored slot.
//! Type validation intentionally handles only the slot's two refinement layers.
use carina_core::schema::{InputPredicate, ValidatedResourceSchema};

pub fn accept(schema: &ValidatedResourceSchema, path: &str, value: &str) -> bool {
    schema.parts().input_rules.iter().all(|rule| {
        if rule.path().segments().collect::<Vec<_>>().join(".") != path {
            return true;
        }
        match rule.parts().predicate {
            InputPredicate::Matches(pattern) => pattern.is_match(value),
            _ => true,
        }
    })
}
