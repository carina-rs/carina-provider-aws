use carina_core::resource::{ConcreteValue, Value};
use carina_core::schema::{AttributeType, TypeIdentity, legacy_validator};
use carina_core::schema::{PatternMatch, PatternSpec};

// Real-AWS probe by the awscc agent in carina-test-001, 2026-11-04:
// HTTP 0200, 200-0299, 200,0302 and gRPC 00-05, 0,05 were rejected.
// HealthCheckPort 08080 was accepted but read back as 8080; reject leading
// zeros in all three types to avoid phantom diffs. Equal range endpoints are accepted.

/// Shared anonymous ELBv2 health-check port: `traffic-port` or canonical decimal 1..=65535.
pub fn elbv2_health_check_port() -> AttributeType {
    AttributeType::refined_string(
        None,
        Some(PatternSpec::new(
            r"^(?:traffic-port|[1-9][0-9]{0,3}|[1-5][0-9]{4}|6[0-4][0-9]{3}|65[0-4][0-9]{2}|655[0-2][0-9]|6553[0-5])\z",
            PatternMatch::Full,
        )),
        None,
        None,
    )
}

pub(crate) fn elbv2_matcher_codes(identity: TypeIdentity, min: u16, max: u16) -> AttributeType {
    AttributeType::refined_string_with_validator(
        identity,
        None,
        None,
        legacy_validator(move |value| {
            if let Value::Concrete(ConcreteValue::String(value)) = value {
                validate_matcher_codes(value.expose_plaintext(), min, max).map_err(Into::into)
            } else {
                Err("Expected string".into())
            }
        }),
        None,
    )
}

fn validate_matcher_codes(codes: &str, min: u16, max: u16) -> Result<(), String> {
    let parse_code = |code: &str| {
        if code.is_empty()
            || !code.bytes().all(|digit| digit.is_ascii_digit())
            || (code.len() > 1 && code.starts_with('0'))
        {
            return None;
        }
        code.parse::<u16>()
            .ok()
            .filter(|code| (min..=max).contains(code))
    };

    let valid = codes.split(',').all(|item| {
        if let Some((start, end)) = item.split_once('-') {
            match (parse_code(start), parse_code(end)) {
                (Some(start), Some(end)) => start <= end,
                _ => false,
            }
        } else {
            parse_code(item).is_some()
        }
    });
    if valid {
        Ok(())
    } else {
        Err(format!(
            "ELBv2 matcher codes must be comma-separated items N or N-N (canonical decimal digits only, no leading zeros), with every N in {min}..={max} and range start <= end; no spaces or empty items"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use carina_core::schema::RawShape;

    #[test]
    fn elbv2_health_check_port_is_anonymous_and_validates_whole_strings() {
        let attr = elbv2_health_check_port();
        let RawShape::String {
            identity,
            pattern: Some(pattern),
            length,
            validate,
            ..
        } = attr.raw_shape()
        else {
            panic!("ELBv2 health-check port must be a patterned String");
        };
        assert!(identity.is_none() && validate.is_none() && length.is_none());
        assert!(pattern.as_str().starts_with('^'));
        assert!(pattern.as_str().ends_with(r"\z"));
        let admitted = crate::tests::rootless(attr);
        for valid in ["traffic-port", "1", "80", "8080", "65535"] {
            let value = Value::Concrete(ConcreteValue::String(valid.to_string().into()));
            assert!(admitted.validate_collect(&value).is_empty(), "{valid:?}");
        }
        for invalid in [
            "",
            "0",
            "000",
            "080",
            "00001",
            "065535",
            // The probe returned 8080 for 08080; require the canonical input.
            "08080",
            "65536",
            "065536",
            "100000",
            "traffic-port\n",
            "traffic-portx",
            "xtraffic-port",
            "Traffic-port",
            "80\n",
            "\n80",
            "80\r",
            "80 ",
            " 80",
            "8 0",
            "80,81",
            "80-81",
            "+80",
            "-1",
            "1.0",
            "1e2",
            "0x50",
            "８０",
            "٨٠",
            "80\0",
        ] {
            let value = Value::Concrete(ConcreteValue::String(invalid.to_string().into()));
            assert!(!admitted.validate_collect(&value).is_empty(), "{invalid:?}");
        }
        assert!(
            !admitted
                .validate_collect(&Value::Concrete(ConcreteValue::Int(80)))
                .is_empty()
        );
    }

    #[test]
    fn elbv2_health_check_port_pattern_covers_every_port() {
        let attr = elbv2_health_check_port();
        let RawShape::String {
            pattern: Some(pattern),
            ..
        } = attr.raw_shape()
        else {
            panic!("health-check port pattern");
        };
        let regex = regex::Regex::new(pattern.as_str()).expect("valid port regex");
        for port in 0..=65536 {
            assert_eq!(
                regex.is_match(&port.to_string()),
                (1..=65535).contains(&port)
            );
            assert!(!regex.is_match(&format!("000{port}")));
        }
    }

    fn assert_matcher_codes(row: crate::AwsType, valid: &[&str], invalid: &[&str]) {
        let admitted = crate::tests::rootless(row.attribute_type());
        for code in valid {
            let value = Value::Concrete(ConcreteValue::String((*code).to_string().into()));
            assert!(
                admitted.validate_collect(&value).is_empty(),
                "{row:?}: {code:?}"
            );
        }
        for code in invalid {
            let value = Value::Concrete(ConcreteValue::String((*code).to_string().into()));
            assert!(
                !admitted.validate_collect(&value).is_empty(),
                "{row:?}: {code:?}"
            );
        }
    }

    #[test]
    fn elbv2_http_matcher_codes_validate_boundaries_lists_and_ranges() {
        assert_matcher_codes(
            crate::AwsType::Elbv2TargetGroupHttpCode,
            &[
                "200",
                "599",
                "200,202,599",
                "200-599",
                "200,201-299,500-599",
                "200-299,302",
                "200-200",
            ],
            &[
                "0200",
                "200-0299",
                "200,0302",
                "0200-0599",
                "0200-200",
                "0200,0599",
                "0000000000000000000000000000000200",
                "199",
                "600",
                "199-200",
                "200-600",
                "600-600",
                "599-200",
                "0201-200",
                "200,199",
                "200,600",
                "",
                ",",
                ",200",
                "200,",
                "200,,201",
                "200, 201",
                "200 ",
                " 200",
                "200 -201",
                "200- 201",
                "200\n",
                "200\r",
                "200\t",
                "2 00",
                "-200",
                "+200",
                "200-",
                "-",
                "200--201",
                "200-201-202",
                "200.0",
                "2e2",
                "0xC8",
                "２００",
                "٢٠٠",
                "abc",
                "200\0",
                "65536",
                "999999999999999999999999",
            ],
        );
    }

    #[test]
    fn elbv2_grpc_matcher_codes_validate_boundaries_lists_and_ranges() {
        assert_matcher_codes(
            crate::AwsType::Elbv2TargetGroupGrpcCode,
            &[
                "0",
                "99",
                "0,1,99",
                "0-99",
                "0,1-5,80-99",
                "0-5",
                "12",
                "0-0",
                "99-99",
            ],
            &[
                "00-05",
                "0,05",
                "00",
                "080",
                "00-099",
                "080-80",
                "00,080,099",
                "0000000000000000000000000000000080",
                "100",
                "-1",
                "0-100",
                "100-100",
                "99-0",
                "080-79",
                "0,100",
                "",
                ",",
                ",0",
                "0,",
                "0,,1",
                "0, 1",
                "0 ",
                " 0",
                "0 -1",
                "0- 1",
                "0\n",
                "0\r",
                "0\t",
                "+0",
                "0-",
                "-",
                "0--1",
                "0-1-2",
                "0.0",
                "1e1",
                "0x0",
                "０",
                "٠",
                "abc",
                "0\0",
                "65536",
                "999999999999999999999999",
            ],
        );
    }
}
