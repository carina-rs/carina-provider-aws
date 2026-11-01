# carina-provider-aws

The AWS provider for [Carina](https://github.com/carina-rs/carina), built on the native AWS SDK for Rust.

Resource schemas are generated from the AWS Smithy models, so attribute names, types, and enum values in your `.crn` files are checked before any API call. The provider is compiled to a WebAssembly component and runs inside the Carina host's sandbox.

> [!CAUTION]
> Carina is experimental. Resource shapes and attributes may change without notice.

## Usage

```crn
provider aws {
  source  = 'carina-rs/aws'
  version = '0.7.4'
  region  = aws.Region.ap_northeast_1
}

let main_vpc = aws.ec2.Vpc {
  cidr_block = '10.0.0.0/16'
}
```

```console
$ carina init
$ carina plan
```

`carina init` resolves the version through the provider registry, verifies the artifact's signature, and records it in `carina-providers.lock`. See [Installation](https://carina-rs.dev/getting-started/installation/) for the other `source` forms.

### Credentials

The Carina host resolves AWS credentials with the standard AWS credential chain (environment, shared config, SSO, and so on) and hands them to the provider, refreshing them when they expire. Select a profile with `AWS_PROFILE`. Carina never starts an interactive login: if an SSO session has expired, run `aws sso login` and retry. A provider-level `assume_role` uses these credentials as its base.

Planning KMS references may require `kms:DescribeKey` for the effective identity,
including its assumed role, and authorization in the key policy. See
[KMS reference planning permissions](docs/registry/index.md#kms-reference-planning-permissions).

## Supported services

ACM, EC2, IAM, Identity Store, CloudWatch Logs, Organizations, Route 53, S3, SQS, and STS.

Per-resource reference documentation is in [`generated-docs/`](generated-docs/), and runnable configurations are in [`examples/`](examples/).

## Development

```console
$ cargo test
$ cargo build -p carina-provider-aws --target wasm32-wasip2 --release
```

Schemas and docs are generated; edit the generator, not the output. See [CODEGEN.md](CODEGEN.md).

| Crate | Role |
|-------|------|
| `carina-provider-aws` | The provider itself |
| `carina-codegen-aws` | Generates resource schemas from the Smithy models |
| `carina-smithy` | Smithy model reader |
| `carina-aws-types` | AWS value types shared with the Cloud Control provider |
| `carina-docgen` | Generates `generated-docs/` |

## Related

- [carina](https://github.com/carina-rs/carina) — CLI, DSL, and plugin host
- [carina-provider-awscc](https://github.com/carina-rs/carina-provider-awscc) — AWS provider built on the Cloud Control API
