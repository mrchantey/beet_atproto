//! The deploy step an atproto handle stack takes after its apply: asking the
//! network whether the record it just published actually resolves.
//!
//! An apply converges a TXT record and stops there. Whether that record makes a
//! handle is a question only a resolver can answer, so it is a verb rather than
//! a resource: idempotent, safe to re-run, and reporting what it found.
mod atproto_handle_probe;
pub use atproto_handle_probe::*;
