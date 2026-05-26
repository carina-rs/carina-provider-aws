//! AWS resource schema definitions

pub mod generated;
pub mod types;

use carina_core::schema::ResourceSchema;

/// Returns all AWS schemas
pub fn all_schemas() -> Vec<ResourceSchema> {
    generated::configs().into_iter().map(|c| c.schema).collect()
}

#[cfg(test)]
mod tests {
    use carina_core::schema::SchemaKind;

    #[test]
    fn configs_register_s3_bucket_under_both_kinds() {
        let configs = super::generated::configs();
        let managed = configs
            .iter()
            .find(|c| c.resource_type_name == "s3.Bucket" && c.schema.kind == SchemaKind::Resource);
        let data_source = configs.iter().find(|c| {
            c.resource_type_name == "s3.Bucket" && c.schema.kind == SchemaKind::DataSource
        });
        assert!(
            managed.is_some(),
            "Managed s3.Bucket missing from configs()"
        );
        assert!(
            data_source.is_some(),
            "DataSource s3.Bucket missing from configs()"
        );
    }
}
