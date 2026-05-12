//! Data source read for `aws.iam.Roles`.
//!
//! Wraps `iam:ListRoles`, paginates to completion, then applies the optional
//! `name_regex` filter client-side (matching Terraform's `data.aws_iam_roles`
//! behavior). Returns `arns` and `names` as parallel `list(String)` outputs.

use std::collections::HashMap;

use aws_sdk_iam::types::Role;
use carina_core::provider::{BoxFuture, ProviderError, ProviderResult};
use carina_core::resource::{ConcreteValue, Resource, State, Value};
use regex::Regex;

use crate::AwsProvider;
use crate::helpers::{sdk_error_message, value_as_str};

impl AwsProvider {
    /// Read `iam.Roles` given a `Resource` with optional `path_prefix` /
    /// `name_regex` inputs.
    pub(crate) fn do_read_iam_roles_data_source(
        &self,
        resource: &Resource,
    ) -> BoxFuture<'_, ProviderResult<State>> {
        let resource = resource.clone();
        Box::pin(async move {
            let path_prefix = resource.get_attr("path_prefix").and_then(value_as_str);
            let name_regex_raw = resource.get_attr("name_regex").and_then(value_as_str);

            let name_filter = match name_regex_raw {
                Some(pattern) => Some(Regex::new(pattern).map_err(|e| {
                    ProviderError::invalid_input(format!(
                        "iam.Roles `name_regex` is not a valid regular expression: {e}"
                    ))
                    .for_resource(resource.id.clone())
                })?),
                None => None,
            };

            let matches = list_matching_roles(
                &self.iam_client,
                path_prefix,
                name_filter.as_ref(),
                &resource,
            )
            .await?;
            Ok(State::existing(
                resource.id.clone(),
                build_attributes(&matches),
            ))
        })
    }
}

/// Stream `ListRoles` to completion, keeping only `(name, arn)` pairs whose
/// `name` matches `name_filter` (if any). `path_prefix` is applied server-
/// side; the regex is applied per page so unmatched roles never accumulate.
async fn list_matching_roles(
    client: &aws_sdk_iam::Client,
    path_prefix: Option<&str>,
    name_filter: Option<&Regex>,
    resource: &Resource,
) -> Result<Vec<(String, String)>, ProviderError> {
    let mut matches: Vec<(String, String)> = Vec::new();
    let mut marker: Option<String> = None;

    loop {
        let mut req = client.list_roles();
        if let Some(p) = path_prefix {
            req = req.path_prefix(p);
        }
        if let Some(m) = &marker {
            req = req.marker(m);
        }
        let resp = req.send().await.map_err(|e| {
            ProviderError::api_error(sdk_error_message("Failed to list IAM roles", &e))
                .for_resource(resource.id.clone())
        })?;

        matches.extend(filter_page(resp.roles(), name_filter));

        if resp.is_truncated() {
            marker = resp.marker().map(str::to_string);
            if marker.is_none() {
                break;
            }
        } else {
            break;
        }
    }

    Ok(matches)
}

/// Apply `name_filter` to one `ListRoles` page, returning the matching
/// `(name, arn)` pairs in input order. Separated out so the per-page filter
/// can be unit-tested without the AWS SDK pagination machinery.
fn filter_page(roles: &[Role], name_filter: Option<&Regex>) -> Vec<(String, String)> {
    roles
        .iter()
        .filter(|r| name_filter.is_none_or(|re| re.is_match(r.role_name())))
        .map(|r| (r.role_name().to_string(), r.arn().to_string()))
        .collect()
}

/// Project filtered `(name, arn)` pairs into the `arns` / `names` aggregated
/// outputs. Pure projection — split out so it can be unit-tested without
/// touching the AWS SDK.
fn build_attributes(matches: &[(String, String)]) -> HashMap<String, Value> {
    let mut arns: Vec<Value> = Vec::with_capacity(matches.len());
    let mut names: Vec<Value> = Vec::with_capacity(matches.len());
    for (name, arn) in matches {
        names.push(Value::Concrete(ConcreteValue::String(name.clone())));
        arns.push(Value::Concrete(ConcreteValue::String(arn.clone())));
    }

    let mut attributes = HashMap::new();
    attributes.insert(
        "arns".to_string(),
        Value::Concrete(ConcreteValue::List(arns)),
    );
    attributes.insert(
        "names".to_string(),
        Value::Concrete(ConcreteValue::List(names)),
    );
    attributes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract_strings(v: &Value) -> Vec<&str> {
        match v {
            Value::Concrete(ConcreteValue::List(items)) => items
                .iter()
                .filter_map(|i| {
                    if let Value::Concrete(ConcreteValue::String(s)) = i {
                        Some(s.as_str())
                    } else {
                        None
                    }
                })
                .collect(),
            _ => panic!("expected list, got {v:?}"),
        }
    }

    fn role(name: &str, arn: &str) -> Role {
        Role::builder()
            .role_name(name)
            .arn(arn)
            .path("/")
            .role_id(format!("AROAEXAMPLE{name}"))
            .create_date(aws_smithy_types::DateTime::from_secs(0))
            .build()
            .unwrap()
    }

    /// In-test driver mirroring `list_matching_roles` for one synthetic page.
    fn filter(pairs: &[(&str, &str)], name_filter: Option<&Regex>) -> Vec<(String, String)> {
        let page: Vec<Role> = pairs.iter().map(|(n, a)| role(n, a)).collect();
        filter_page(&page, name_filter)
    }

    #[test]
    fn build_attributes_with_no_filter_returns_all_roles() {
        let pairs = filter(
            &[
                ("AdminRole", "arn:aws:iam::123:role/AdminRole"),
                ("ReadOnly", "arn:aws:iam::123:role/ReadOnly"),
            ],
            None,
        );
        let attrs = build_attributes(&pairs);

        assert_eq!(
            extract_strings(&attrs["names"]),
            vec!["AdminRole", "ReadOnly"]
        );
        assert_eq!(
            extract_strings(&attrs["arns"]),
            vec![
                "arn:aws:iam::123:role/AdminRole",
                "arn:aws:iam::123:role/ReadOnly",
            ]
        );
    }

    #[test]
    fn regex_filter_keeps_only_matches() {
        let re = Regex::new(r"^AWSReservedSSO_AdministratorAccess_[0-9a-f]{16}$").unwrap();
        let pairs = filter(
            &[
                (
                    "AWSReservedSSO_AdministratorAccess_1ade6440f2d3bcdd",
                    "arn:aws:iam::123:role/aws-reserved/sso.amazonaws.com/AWSReservedSSO_AdministratorAccess_1ade6440f2d3bcdd",
                ),
                (
                    "AWSReservedSSO_ReadOnly_abc123def4567890",
                    "arn:aws:iam::123:role/aws-reserved/sso.amazonaws.com/AWSReservedSSO_ReadOnly_abc123def4567890",
                ),
                ("AdminRole", "arn:aws:iam::123:role/AdminRole"),
            ],
            Some(&re),
        );
        let attrs = build_attributes(&pairs);

        assert_eq!(
            extract_strings(&attrs["names"]),
            vec!["AWSReservedSSO_AdministratorAccess_1ade6440f2d3bcdd"]
        );
        let arns = extract_strings(&attrs["arns"]);
        assert_eq!(arns.len(), 1);
        assert!(arns[0].ends_with("/AWSReservedSSO_AdministratorAccess_1ade6440f2d3bcdd"));
    }

    #[test]
    fn empty_input_yields_empty_lists() {
        let attrs = build_attributes(&[]);
        assert_eq!(extract_strings(&attrs["names"]), Vec::<&str>::new());
        assert_eq!(extract_strings(&attrs["arns"]), Vec::<&str>::new());
    }

    #[test]
    fn regex_with_no_match_yields_empty_lists() {
        let re = Regex::new(r"^NoMatch$").unwrap();
        let pairs = filter(
            &[("AdminRole", "arn:aws:iam::123:role/AdminRole")],
            Some(&re),
        );
        let attrs = build_attributes(&pairs);
        assert_eq!(extract_strings(&attrs["names"]), Vec::<&str>::new());
        assert_eq!(extract_strings(&attrs["arns"]), Vec::<&str>::new());
    }

    /// names[i] and arns[i] must refer to the same role even when
    /// intervening roles are filtered out.
    #[test]
    fn preserves_name_arn_pairing_under_filter() {
        let re = Regex::new(r"^Keep").unwrap();
        let pairs = filter(
            &[
                ("Keep1", "arn:aws:iam::123:role/Keep1"),
                ("Skip", "arn:aws:iam::123:role/Skip"),
                ("Keep2", "arn:aws:iam::123:role/Keep2"),
            ],
            Some(&re),
        );
        let attrs = build_attributes(&pairs);

        assert_eq!(extract_strings(&attrs["names"]), vec!["Keep1", "Keep2"]);
        assert_eq!(
            extract_strings(&attrs["arns"]),
            vec!["arn:aws:iam::123:role/Keep1", "arn:aws:iam::123:role/Keep2"]
        );
    }

    /// Mirrors `list_matching_roles`'s extend-per-page accumulation across
    /// multiple synthetic ListRoles pages. Catches regressions where a
    /// refactor would accidentally overwrite earlier pages or drop ordering.
    #[test]
    fn multi_page_accumulation_preserves_order_across_pages() {
        let re = Regex::new(r"^Keep").unwrap();
        let page1 = vec![
            role("Keep1", "arn:aws:iam::123:role/Keep1"),
            role("Skip1", "arn:aws:iam::123:role/Skip1"),
        ];
        let page2 = vec![
            role("Keep2", "arn:aws:iam::123:role/Keep2"),
            role("Keep3", "arn:aws:iam::123:role/Keep3"),
        ];

        let mut matches: Vec<(String, String)> = Vec::new();
        matches.extend(filter_page(&page1, Some(&re)));
        matches.extend(filter_page(&page2, Some(&re)));

        let attrs = build_attributes(&matches);
        assert_eq!(
            extract_strings(&attrs["names"]),
            vec!["Keep1", "Keep2", "Keep3"]
        );
        assert_eq!(
            extract_strings(&attrs["arns"]),
            vec![
                "arn:aws:iam::123:role/Keep1",
                "arn:aws:iam::123:role/Keep2",
                "arn:aws:iam::123:role/Keep3",
            ]
        );
    }

    /// `path_prefix` is enforced server-side, so for the runtime filter it
    /// is a no-op. This test guards the invariant: passing a `path_prefix`-
    /// filtered page through `filter_page` with a separate `name_regex`
    /// behaves as `name_regex` alone.
    #[test]
    fn path_prefix_and_name_regex_compose_independently() {
        let re = Regex::new(r"^AWSReservedSSO_AdministratorAccess_").unwrap();
        // Simulated server-side path_prefix filter — only SSO-pathed roles in the page.
        let page = vec![
            role(
                "AWSReservedSSO_AdministratorAccess_1ade6440f2d3bcdd",
                "arn:aws:iam::123:role/aws-reserved/sso.amazonaws.com/AWSReservedSSO_AdministratorAccess_1ade6440f2d3bcdd",
            ),
            role(
                "AWSReservedSSO_ReadOnly_abc123def4567890",
                "arn:aws:iam::123:role/aws-reserved/sso.amazonaws.com/AWSReservedSSO_ReadOnly_abc123def4567890",
            ),
        ];

        let matches = filter_page(&page, Some(&re));
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].0,
            "AWSReservedSSO_AdministratorAccess_1ade6440f2d3bcdd"
        );
    }
}
