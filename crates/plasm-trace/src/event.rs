//! Wall-clock envelope for each segment (SSE patches + durable replay ordering).

use serde::{Deserialize, Serialize};

use crate::TraceSegment;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraceEvent {
    pub emitted_at_ms: u64,
    #[serde(flatten)]
    pub segment: TraceSegment,
}

impl TraceEvent {
    pub fn at(emitted_at_ms: u64, segment: TraceSegment) -> Self {
        Self {
            emitted_at_ms,
            segment,
        }
    }
}
