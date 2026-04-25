//! Code Mode facade: structured `facade_delta` + TypeScript declaration generation from CGS and exposure.
//!
//! QuickJS and **genco** are compile-time / test dependencies for the sandbox & JS harness boundary.

mod delta;
mod gen;
mod quickjs_bootstrap;

#[cfg(test)]
mod snapshot_tests;

pub use delta::{
    CatalogAliasRecord, ExposedSet, FacadeDeltaV1, FacadeInputParameter, FacadeInvokePreflight,
    FacadeOutputSurface, QualifiedEntitySurface, TypeScriptCodeArtifacts,
};
pub use gen::{build_code_facade, CatalogAliasMap, FacadeGenRequest};
pub use quickjs_bootstrap::{quickjs_runtime_from_facade_delta, quickjs_runtime_module_bootstrap};
