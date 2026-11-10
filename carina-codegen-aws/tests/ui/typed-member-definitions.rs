fn main() {
    use carina_aws_types::{AwsType, CoreIdentity};
    use carina_core::schema::AttributeType;
    carina_aws_types::with_member_identity(AttributeType::string(), AwsType::VpcId.definition());
    carina_aws_types::with_member_identity(AttributeType::string(), CoreIdentity::Email.definition());
    carina_aws_types::try_with_member_identity(AttributeType::int(), AwsType::Ec2AmazonSideAsn.definition()).unwrap();
    carina_aws_types::with_member_reference(AttributeType::string(), AwsType::KmsKeyReference.definition());
}
