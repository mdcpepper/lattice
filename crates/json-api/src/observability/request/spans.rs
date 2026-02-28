//! HTTP span helpers.

use uuid::Uuid;

#[derive(Debug, Clone)]
pub(super) struct RequestSpanName {
    pub(super) otel_path: String,
    pub(super) otel_span_name: String,
}

pub(super) fn request_span_name(method: &str, path: &str) -> RequestSpanName {
    let otel_path = normalise_path_for_span_name(path);
    let otel_span_name = format!("{method} {otel_path}");

    RequestSpanName {
        otel_path,
        otel_span_name,
    }
}

fn normalise_path_for_span_name(path: &str) -> String {
    if path == "/" {
        return "/".to_owned();
    }

    let mut normalised = String::from("/");

    for (index, segment) in path.trim_start_matches('/').split('/').enumerate() {
        if index > 0 {
            normalised.push('/');
        }

        if Uuid::parse_str(segment).is_ok() {
            normalised.push_str("{uuid}");
        } else {
            normalised.push_str(segment);
        }
    }

    normalised
}
