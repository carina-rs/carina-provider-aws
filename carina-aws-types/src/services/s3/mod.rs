mod cors;
mod encryption;
mod lifecycle;
mod notification;
mod object_key_format;
mod object_lock;
mod ownership;
mod replication;
mod versioning;
mod website;

pub use cors::*;
pub use encryption::*;
pub use lifecycle::*;
pub use notification::*;
pub use object_key_format::*;
pub use object_lock::*;
pub use ownership::*;
pub use replication::*;
pub use versioning::*;
pub use website::*;

// Anonymous constraints shared by the S3 helper constructors. No catalog row
// is needed: these shapes neither introduce an identity nor need a callback.
fn rule_id() -> carina_core::schema::AttributeType {
    carina_core::schema::AttributeType::string_member(
        carina_core::schema::StringIdentityLayer::new(None, None, None, None),
        carina_core::schema::StringMemberConstraints::new(None, Some((Some(0), Some(255)))),
    )
}

fn nonempty_list(
    element: carina_core::schema::AttributeType,
) -> carina_core::schema::AttributeType {
    carina_core::schema::AttributeType::refined_list(
        element,
        true,
        Some((Some(1), None)),
        carina_core::schema::ListElements::AllowDuplicates,
    )
}

fn rule_list(
    element: carina_core::schema::AttributeType,
    minimum: u64,
    maximum: u64,
) -> carina_core::schema::AttributeType {
    carina_core::schema::AttributeType::refined_list(
        element,
        true,
        Some((Some(minimum), Some(maximum))),
        carina_core::schema::ListElements::AllowDuplicates,
    )
}
