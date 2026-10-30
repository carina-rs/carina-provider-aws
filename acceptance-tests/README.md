# Acceptance Tests

## Persistent run registry and cleanup

Stateful acceptance tests register each fixture under
`${CARINA_ACCEPTANCE_HOME:-$HOME/.cache/carina-acceptance}/carina-provider-aws/runs/<run-id>/`.
Each entry keeps its injected `main.crn`, local Carina state, and metadata for
the fixture and AWS account. Temporary log directories may be removed, but a
registry entry is removed only after `carina state show --json` reports absent
state or an empty resource list.

Use `./acceptance-tests/run-tests.sh cleanup` to destroy every registered
entry. `--accounts START-END` restricts cleanup to those accounts, and any
remaining arguments filter the stored fixture path by substring. For example:

```console
./acceptance-tests/run-tests.sh cleanup --accounts 3-3 ec2_vpc
```

Cleanup uses the account recorded in each entry. Unusable entries and failed or
incomplete destroys produce a non-zero exit and remain registered, with their
paths printed for investigation. If the registry has no matching entries, the
command reports `No registered entries`.

Custom suites that are run directly also use this registry. Set `AWS_PROFILE`
to the account profile, or set `CARINA_ACCEPTANCE_ACCOUNT` to that profile name
when credentials were exported separately, so a later cleanup can select the
same account.

## Known coverage gaps

The following resource types intentionally do not have runnable acceptance
fixtures:

- `aws.organizations.Organization`: skipped because the shared
  `carina-test-00X` pool accounts are already members of an AWS Organization,
  and `CreateOrganization` requires the caller account not to belong to one.
- `aws.organizations.Account`: skipped because account closure has AWS cooldowns
  and quotas, and closed accounts remain visible in the organization for 90 days,
  making repeated acceptance runs unsuitable for the shared pool.
- `aws.identitystore.User`: skipped because it requires an IAM Identity Center
  identity store, which exists in the organization management account rather
  than the pool accounts used by these tests.
