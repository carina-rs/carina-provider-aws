# Acceptance Tests

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
