//! Synthetic REST backend for testing and development.
//!
//! Provides a configurable HTTP server that serves resources
//! and supports filtering for integration testing.

pub mod error;
pub mod handlers;
pub mod server;
pub mod store;

pub use error::*;
pub use handlers::*;
pub use server::*;
pub use store::*;

// Re-export the router creation function for easier use
pub use server::create_router;
