use serde_json::Value;

use crate::{ContentAnnotations, ContentAudience, ContentIcon, ContentIconTheme, ContentPriority};

use super::time::parse_rfc3339;

pub(crate) fn annotations(value: &Value) -> Option<ContentAnnotations> {
    let fields = value.get("annotations")?.as_object()?;
    let audience = fields
        .get("audience")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(content_audience)
        .collect();
    let last_modified = fields
        .get("lastModified")
        .or_else(|| fields.get("last_modified"))
        .and_then(Value::as_str)
        .and_then(parse_rfc3339);
    let priority = fields
        .get("priority")
        .and_then(Value::as_f64)
        .and_then(ContentPriority::new);

    Some(ContentAnnotations {
        audience,
        last_modified,
        priority,
    })
}

pub(crate) fn icons(value: &Value) -> Vec<ContentIcon> {
    value
        .get("icons")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|icon| {
            let src = icon.get("src").and_then(Value::as_str)?.to_owned();
            let mime_type = icon
                .get("mimeType")
                .or_else(|| icon.get("mime_type"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            let sizes = icon
                .get("sizes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            let theme = icon
                .get("theme")
                .and_then(Value::as_str)
                .map(content_icon_theme);
            Some(ContentIcon {
                src,
                mime_type,
                sizes,
                theme,
            })
        })
        .collect()
}

fn content_audience(value: &str) -> ContentAudience {
    match value {
        "user" => ContentAudience::User,
        "assistant" => ContentAudience::Assistant,
        value => ContentAudience::Other(value.to_owned()),
    }
}

fn content_icon_theme(value: &str) -> ContentIconTheme {
    match value {
        "light" => ContentIconTheme::Light,
        "dark" => ContentIconTheme::Dark,
        value => ContentIconTheme::Other(value.to_owned()),
    }
}
