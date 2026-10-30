# Acceptance Tests

## Persistent run registry and cleanup

Stateful acceptance tests register each fixture under
`${CARINA_ACCEPTANCE_HOME:-$HOME/.cache/carina-acceptance}/carina-provider-aws/runs/<run-id>/`.
Each entry keeps its injected `main.crn`, local Carina state, and metadata for
the fixture, AWS account, and creating host. Each run also records its creating
host, PID, and process start time so PID reuse is not mistaken for a live run.
Temporary log directories may be removed, but a registry entry is removed only
after `carina state show --json` reports absent state or an empty resource list.

Use `./acceptance-tests/run-tests.sh cleanup` to destroy every registered
entry. `--accounts START-END` restricts cleanup to those accounts, and any
remaining arguments filter the stored fixture path by substring. For example:

```console
./acceptance-tests/run-tests.sh cleanup --accounts 3-3 ec2_vpc
```

Cleanup uses the account recorded in each entry. Unusable entries remain
registered and produce a non-zero exit: without fixture filters they are
validation failures (exit status 1), while unusable metadata that cannot be
matched against a fixture filter is skipped (exit status 3). Failed or
incomplete destroys also produce exit status 1 and remain registered, with
their paths printed for investigation. Runs whose creating process is still
alive on this host are reported as in progress and skipped. Runs created on
another host are also skipped and are never destroyed automatically. Destroy
or validation failures take precedence over skipped entries. If entries exist
but none match the requested accounts or fixture filters, cleanup reports `No
registry entries match`; an empty registry reports `No registered entries`.

Custom suites that are run directly also use this registry. Set `AWS_PROFILE`
to the account profile so the suite uses and records the same account for a
later cleanup.

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
