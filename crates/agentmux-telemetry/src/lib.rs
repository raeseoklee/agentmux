pub const REQUIRED_SPANS: &[&str] = &[
    "app.startup",
    "core.startup",
    "ipc.request",
    "session.spawn",
    "session.attach",
    "session.input",
    "session.resize",
    "backend.read",
    "output.batch",
    "renderer.write",
    "workspace.switch",
    "recovery.attach",
];

pub const REQUIRED_COUNTERS: &[&str] = &[
    "active.workspaces",
    "active.panes",
    "active.surfaces",
    "active.sessions",
    "active.backend_attachments",
    "session.bytes_read",
    "surface.bytes_rendered",
    "hidden.bytes_buffered",
    "output.batches_sent",
    "output.batches_dropped",
    "ipc.requests",
    "ipc.errors",
    "backend.reconnects",
];

#[derive(Clone, Debug, PartialEq)]
pub struct MetricSample {
    pub name: String,
    pub value: f64,
    pub labels: Vec<(String, String)>,
}

impl MetricSample {
    pub fn new(name: impl Into<String>, value: f64) -> Self {
        Self {
            name: name.into(),
            value,
            labels: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_spans_include_input_and_output_paths() {
        assert!(REQUIRED_SPANS.contains(&"session.input"));
        assert!(REQUIRED_SPANS.contains(&"output.batch"));
    }
}
