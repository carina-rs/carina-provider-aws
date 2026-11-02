# AWS Provider

The AWS provider manages AWS resources with credentials resolved by the Carina host. Configure it with a `provider aws` block, then declare resources with names such as `aws.s3.Bucket`. Use a `let` binding when another resource needs to reference its attributes.

```crn
provider aws {
  region = aws.Region.ap_northeast_1
}

let bucket = aws.s3.Bucket {
  bucket = 'carina-example-bucket'
}
```

## Authentication

The host uses the standard AWS credential chain, including environment variables,
named profiles in `~/.aws`, IAM Identity Center (SSO), ECS and EC2 instance roles.
Set `AWS_PROFILE` to select a profile. The host supplies base credentials and their
expiry to the provider and refreshes them when needed.

Carina does not start an interactive login. If an SSO session has expired, run
`aws sso login --profile <profile>` and retry. For example:

```console
aws sso login --profile development
AWS_PROFILE=development carina plan .
```

The provider's service region comes from its `region` attribute, defaulting to
`aws.Region.ap_northeast_1`; a profile's region does not change it.

## KMS reference planning permissions

When a KMS reference differs from the service's returned value, planning compares
the resolved key ARNs. Key IDs, alias names, and alias ARNs require
`kms:DescribeKey`; a key ARN needs no lookup. Validation and LSP diagnostics
remain offline. Identical values and deferred values need no lookup.

Grant this permission to the routed provider instance's effective credentials,
including its assumed role. The key policy must authorize the lookup;
cross-account references also require the caller's IAM policy to allow it.
Bare IDs and aliases use the configured provider region. Qualified ARNs keep
their own region and account. Validation accepts `aws`, `aws-cn`, `aws-us-gov`,
`aws-iso`, `aws-iso-b`, `aws-iso-e`, `aws-iso-f`, and `aws-eusc`. An ARN in a
different partition from the configured provider region aborts planning as an
invalid reference.

This permission is independent of advisory `--check-iam`. A used access denial,
transient failure, or invalid reference aborts planning. A missing desired key
also aborts planning; a missing current key produces a difference so an update
can repair it. Resolving `alias/aws/<service>` calls `DescribeKey` directly and
can initialize the service's AWS-managed key during planning.

Resolved ARNs are comparison values only; payloads and state retain their raw
values. See the [AWS credentials guide](https://carina-rs.dev/guides/aws-credentials/#kms-reference-planning-permissions).

## Configuration

| Attribute | Type | Description |
| --- | --- | --- |
| `region` | `aws.Region` enum | Region used by every AWS service client. It defaults to `aws.Region.ap_northeast_1`. Region identifiers use underscores in the DSL, for example `aws.Region.us_east_1`. |
| `allowed_account_ids` | Unordered list of strings | When non-empty, initialization fails unless the effective AWS caller account is in the list. A determinable account in `assume_role.role_arn` must also be listed. |
| `forbidden_account_ids` | Unordered list of strings | When non-empty, initialization fails if the effective AWS caller account is in the list. This check takes precedence over the allow-list. |
| `assume_role` | `AssumeRole` struct | Uses STS to assume a role with the host-supplied base credentials; all provider operations then use the assumed-role credentials. |

`assume_role` accepts these fields:

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `role_arn` | String | Yes | IAM role ARN to assume. |
| `session_name` | String | No | STS session name for the assumed-role session. |
| `external_id` | String | No | External ID required by the role's trust policy. |
| `duration` | Duration | No | Session duration, such as `30min` or `1h`. STS accepts 15 minutes up to the role's maximum session duration. |
