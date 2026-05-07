use std::collections::HashMap;

use aws_sdk_s3::types::{
    Event, EventBridgeConfiguration, FilterRule, FilterRuleName, LambdaFunctionConfiguration,
    NotificationConfiguration, NotificationConfigurationFilter, QueueConfiguration, S3KeyFilter,
    TopicConfiguration,
};
use carina_core::provider::{ProviderError, ProviderResult};
use carina_core::resource::{Resource, ResourceId, State, Value};
use indexmap::IndexMap;

use crate::AwsProvider;
use crate::helpers::{require_string_attr, sdk_error_message};
use crate::services::s3::bucket::is_s3_not_configured_error;

impl AwsProvider {
    pub(crate) async fn read_s3_bucket_notification_configuration(
        &self,
        id: &ResourceId,
        identifier: Option<&str>,
    ) -> ProviderResult<State> {
        let Some(bucket) = identifier else {
            return Ok(State::not_found(id.clone()));
        };

        let result = self
            .s3_client
            .get_bucket_notification_configuration()
            .bucket(bucket)
            .send()
            .await;

        match result {
            Ok(output) => {
                let mut attributes = HashMap::new();
                attributes.insert("bucket".to_string(), Value::String(bucket.to_string()));

                let topics: Vec<Value> = output
                    .topic_configurations()
                    .iter()
                    .map(topic_to_value)
                    .collect();
                if !topics.is_empty() {
                    attributes.insert("topic_configurations".to_string(), Value::List(topics));
                }
                let queues: Vec<Value> = output
                    .queue_configurations()
                    .iter()
                    .map(queue_to_value)
                    .collect();
                if !queues.is_empty() {
                    attributes.insert("queue_configurations".to_string(), Value::List(queues));
                }
                let lambdas: Vec<Value> = output
                    .lambda_function_configurations()
                    .iter()
                    .map(lambda_to_value)
                    .collect();
                if !lambdas.is_empty() {
                    attributes.insert(
                        "lambda_function_configurations".to_string(),
                        Value::List(lambdas),
                    );
                }
                if output.event_bridge_configuration().is_some() {
                    attributes.insert(
                        "event_bridge_configuration".to_string(),
                        Value::Map(IndexMap::new()),
                    );
                }

                Ok(State::existing(id.clone(), attributes).with_identifier(bucket.to_string()))
            }
            Err(e) => {
                if is_s3_not_configured_error(&e, "NoSuchBucket") {
                    return Ok(State::not_found(id.clone()));
                }
                Err(ProviderError::api_error(sdk_error_message(
                    "Failed to get bucket notification configuration",
                    &e,
                ))
                .for_resource(id.clone()))
            }
        }
    }

    pub(crate) async fn create_s3_bucket_notification_configuration(
        &self,
        resource: Resource,
    ) -> ProviderResult<State> {
        let bucket = require_string_attr(&resource, "bucket")?;
        self.put_s3_bucket_notification(&resource.id, &bucket, &resource)
            .await
    }

    pub(crate) async fn update_s3_bucket_notification_configuration(
        &self,
        id: ResourceId,
        identifier: &str,
        _from: &State,
        to: Resource,
    ) -> ProviderResult<State> {
        self.put_s3_bucket_notification(&id, identifier, &to).await
    }

    async fn put_s3_bucket_notification(
        &self,
        id: &ResourceId,
        bucket: &str,
        resource: &Resource,
    ) -> ProviderResult<State> {
        let topics = match resource.get_attr("topic_configurations") {
            Some(Value::List(items)) => items
                .iter()
                .map(|v| build_topic(id, v))
                .collect::<ProviderResult<Vec<_>>>()?,
            _ => Vec::new(),
        };
        let queues = match resource.get_attr("queue_configurations") {
            Some(Value::List(items)) => items
                .iter()
                .map(|v| build_queue(id, v))
                .collect::<ProviderResult<Vec<_>>>()?,
            _ => Vec::new(),
        };
        let lambdas = match resource.get_attr("lambda_function_configurations") {
            Some(Value::List(items)) => items
                .iter()
                .map(|v| build_lambda(id, v))
                .collect::<ProviderResult<Vec<_>>>()?,
            _ => Vec::new(),
        };
        let event_bridge = resource
            .get_attr("event_bridge_configuration")
            .map(|_| EventBridgeConfiguration::builder().build());

        let mut config = NotificationConfiguration::builder()
            .set_topic_configurations(if topics.is_empty() {
                None
            } else {
                Some(topics)
            })
            .set_queue_configurations(if queues.is_empty() {
                None
            } else {
                Some(queues)
            })
            .set_lambda_function_configurations(if lambdas.is_empty() {
                None
            } else {
                Some(lambdas)
            });
        if let Some(eb) = event_bridge {
            config = config.event_bridge_configuration(eb);
        }
        let config = config.build();

        self.s3_client
            .put_bucket_notification_configuration()
            .bucket(bucket)
            .notification_configuration(config)
            .send()
            .await
            .map_err(|e| {
                ProviderError::api_error(sdk_error_message(
                    "Failed to put bucket notification configuration",
                    &e,
                ))
                .for_resource(id.clone())
            })?;

        self.read_s3_bucket_notification_configuration(id, Some(bucket))
            .await
    }

    pub(crate) async fn delete_s3_bucket_notification_configuration_idempotent(
        &self,
        id: ResourceId,
        identifier: &str,
    ) -> ProviderResult<()> {
        let empty = NotificationConfiguration::builder().build();
        let result = self
            .s3_client
            .put_bucket_notification_configuration()
            .bucket(identifier)
            .notification_configuration(empty)
            .send()
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) if is_s3_not_configured_error(&e, "NoSuchBucket") => Ok(()),
            Err(e) => Err(ProviderError::api_error(sdk_error_message(
                "Failed to clear bucket notification configuration",
                &e,
            ))
            .for_resource(id.clone())),
        }
    }
}

fn build_topic(id: &ResourceId, value: &Value) -> ProviderResult<TopicConfiguration> {
    let Value::Map(map) = value else {
        return Err(
            ProviderError::invalid_input("topic_configurations entry must be a map")
                .for_resource(id.clone()),
        );
    };
    let topic_arn = require_string_field(id, map, "topic_arn", "topic_configurations")?;
    let events = build_events(id, map, "topic_configurations")?;

    let mut builder = TopicConfiguration::builder()
        .topic_arn(topic_arn)
        .set_events(Some(events));
    if let Some(Value::String(s)) = map.get("id") {
        builder = builder.id(s);
    }
    if let Some(filter) = build_filter(id, map, "topic_configurations")? {
        builder = builder.filter(filter);
    }
    builder.build().map_err(|e| {
        ProviderError::api_error(sdk_error_message("Failed to build TopicConfiguration", &e))
            .for_resource(id.clone())
    })
}

fn build_queue(id: &ResourceId, value: &Value) -> ProviderResult<QueueConfiguration> {
    let Value::Map(map) = value else {
        return Err(
            ProviderError::invalid_input("queue_configurations entry must be a map")
                .for_resource(id.clone()),
        );
    };
    let queue_arn = require_string_field(id, map, "queue_arn", "queue_configurations")?;
    let events = build_events(id, map, "queue_configurations")?;

    let mut builder = QueueConfiguration::builder()
        .queue_arn(queue_arn)
        .set_events(Some(events));
    if let Some(Value::String(s)) = map.get("id") {
        builder = builder.id(s);
    }
    if let Some(filter) = build_filter(id, map, "queue_configurations")? {
        builder = builder.filter(filter);
    }
    builder.build().map_err(|e| {
        ProviderError::api_error(sdk_error_message("Failed to build QueueConfiguration", &e))
            .for_resource(id.clone())
    })
}

fn build_lambda(id: &ResourceId, value: &Value) -> ProviderResult<LambdaFunctionConfiguration> {
    let Value::Map(map) = value else {
        return Err(ProviderError::invalid_input(
            "lambda_function_configurations entry must be a map",
        )
        .for_resource(id.clone()));
    };
    let lambda_arn = require_string_field(
        id,
        map,
        "lambda_function_arn",
        "lambda_function_configurations",
    )?;
    let events = build_events(id, map, "lambda_function_configurations")?;

    let mut builder = LambdaFunctionConfiguration::builder()
        .lambda_function_arn(lambda_arn)
        .set_events(Some(events));
    if let Some(Value::String(s)) = map.get("id") {
        builder = builder.id(s);
    }
    if let Some(filter) = build_filter(id, map, "lambda_function_configurations")? {
        builder = builder.filter(filter);
    }
    builder.build().map_err(|e| {
        ProviderError::api_error(sdk_error_message(
            "Failed to build LambdaFunctionConfiguration",
            &e,
        ))
        .for_resource(id.clone())
    })
}

fn require_string_field(
    id: &ResourceId,
    map: &IndexMap<String, Value>,
    field: &str,
    parent: &str,
) -> ProviderResult<String> {
    match map.get(field) {
        Some(Value::String(s)) => Ok(s.clone()),
        _ => Err(
            ProviderError::invalid_input(format!("{parent}.{field} is required"))
                .for_resource(id.clone()),
        ),
    }
}

fn build_events(
    id: &ResourceId,
    map: &IndexMap<String, Value>,
    parent: &str,
) -> ProviderResult<Vec<Event>> {
    match map.get("events") {
        Some(Value::List(items)) => items
            .iter()
            .map(|v| match v {
                Value::String(s) => Ok(Event::from(s.as_str())),
                _ => Err(ProviderError::invalid_input(format!(
                    "{parent}.events must be a list of strings"
                ))
                .for_resource(id.clone())),
            })
            .collect(),
        _ => Err(
            ProviderError::invalid_input(format!("{parent}.events is required"))
                .for_resource(id.clone()),
        ),
    }
}

fn build_filter(
    id: &ResourceId,
    map: &IndexMap<String, Value>,
    parent: &str,
) -> ProviderResult<Option<NotificationConfigurationFilter>> {
    let Some(Value::Map(filter_map)) = map.get("filter") else {
        return Ok(None);
    };
    let Some(Value::List(rules)) = filter_map.get("filter_rules") else {
        return Ok(Some(NotificationConfigurationFilter::builder().build()));
    };

    let sdk_rules: Vec<FilterRule> = rules
        .iter()
        .map(|v| {
            let Value::Map(rule_map) = v else {
                return Err(ProviderError::invalid_input(format!(
                    "{parent}.filter.filter_rules entry must be a map"
                ))
                .for_resource(id.clone()));
            };
            let name = require_string_field(id, rule_map, "name", "filter.filter_rules")?;
            let value = require_string_field(id, rule_map, "value", "filter.filter_rules")?;
            Ok(FilterRule::builder()
                .name(FilterRuleName::from(name.as_str()))
                .value(value)
                .build())
        })
        .collect::<ProviderResult<Vec<_>>>()?;

    let key = S3KeyFilter::builder()
        .set_filter_rules(Some(sdk_rules))
        .build();
    Ok(Some(
        NotificationConfigurationFilter::builder().key(key).build(),
    ))
}

fn topic_to_value(t: &TopicConfiguration) -> Value {
    let mut m = IndexMap::new();
    if let Some(s) = t.id() {
        m.insert("id".to_string(), Value::String(s.to_string()));
    }
    m.insert(
        "topic_arn".to_string(),
        Value::String(t.topic_arn().to_string()),
    );
    m.insert("events".to_string(), events_to_value(t.events()));
    if let Some(f) = t.filter() {
        m.insert("filter".to_string(), filter_to_value(f));
    }
    Value::Map(m)
}

fn queue_to_value(q: &QueueConfiguration) -> Value {
    let mut m = IndexMap::new();
    if let Some(s) = q.id() {
        m.insert("id".to_string(), Value::String(s.to_string()));
    }
    m.insert(
        "queue_arn".to_string(),
        Value::String(q.queue_arn().to_string()),
    );
    m.insert("events".to_string(), events_to_value(q.events()));
    if let Some(f) = q.filter() {
        m.insert("filter".to_string(), filter_to_value(f));
    }
    Value::Map(m)
}

fn lambda_to_value(l: &LambdaFunctionConfiguration) -> Value {
    let mut m = IndexMap::new();
    if let Some(s) = l.id() {
        m.insert("id".to_string(), Value::String(s.to_string()));
    }
    m.insert(
        "lambda_function_arn".to_string(),
        Value::String(l.lambda_function_arn().to_string()),
    );
    m.insert("events".to_string(), events_to_value(l.events()));
    if let Some(f) = l.filter() {
        m.insert("filter".to_string(), filter_to_value(f));
    }
    Value::Map(m)
}

fn events_to_value(events: &[Event]) -> Value {
    Value::List(
        events
            .iter()
            .map(|e| Value::String(e.as_str().to_string()))
            .collect(),
    )
}

fn filter_to_value(f: &NotificationConfigurationFilter) -> Value {
    let mut outer = IndexMap::new();
    if let Some(key) = f.key() {
        let rules: Vec<Value> = key
            .filter_rules()
            .iter()
            .map(|r| {
                let mut m = IndexMap::new();
                if let Some(n) = r.name() {
                    m.insert("name".to_string(), Value::String(n.as_str().to_string()));
                }
                if let Some(v) = r.value() {
                    m.insert("value".to_string(), Value::String(v.to_string()));
                }
                Value::Map(m)
            })
            .collect();
        outer.insert("filter_rules".to_string(), Value::List(rules));
    }
    Value::Map(outer)
}
