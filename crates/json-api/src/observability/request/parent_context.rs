//! Parent trace context extraction from HTTP headers.

use opentelemetry::{Context, global, propagation::Extractor, trace::TraceContextExt as _};
use salvo::http::{HeaderMap, HeaderName};

pub(super) fn extract_parent_context(headers: &HeaderMap) -> Option<Context> {
    let extractor = HeaderExtractor::new(headers);

    global::get_text_map_propagator(|propagator| {
        // Use a fresh base context so missing trace headers don't inherit the
        // currently active in-process span chain.
        let context = propagator.extract_with_context(&Context::new(), &extractor);
        let span = context.span();
        let span_context = span.span_context();

        span_context.is_valid().then_some(context)
    })
}

#[derive(Debug)]
struct HeaderExtractor<'a> {
    headers: &'a HeaderMap,
}

impl<'a> HeaderExtractor<'a> {
    fn new(headers: &'a HeaderMap) -> Self {
        Self { headers }
    }
}

impl Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        let value = self.headers.get(key)?;
        value.to_str().ok()
    }

    fn keys(&self) -> Vec<&str> {
        self.headers.keys().map(HeaderName::as_str).collect()
    }
}
