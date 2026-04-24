#[cfg(feature = "llm")]
#[allow(
    clippy::empty_line_after_doc_comments,
    clippy::new_without_default,
    clippy::map_clone,
    clippy::unwrap_or_default,
    clippy::derivable_impls
)]
include!("../baml_client/mod.rs");

#[cfg(not(feature = "llm"))]
pub use stub::*;

#[cfg(not(feature = "llm"))]
mod stub {
    use std::collections::HashMap;
    use std::error::Error;
    use std::fmt;

    // Mirrors the generated BAML API surface used by the eval and REPL crates.
    // Keep these symbols in sync with baml_src/query_expr.baml:
    // ClientRegistry, types::{PlanChatTurn, Union2KassistantOrKuser},
    // and sync_client::{B, TranslatePlanOutput}.
    pub fn init() {}

    #[derive(Debug, Clone, Default)]
    pub struct ClientRegistry;

    impl ClientRegistry {
        pub fn new() -> Self {
            Self
        }

        pub fn add_llm_client(
            &mut self,
            _name: &str,
            _provider: &str,
            _options: HashMap<String, serde_json::Value>,
        ) {
        }

        pub fn set_primary_client(&mut self, _name: &str) {}
    }

    #[derive(Debug)]
    pub struct MissingBamlClient;

    impl fmt::Display for MissingBamlClient {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(
                "LLM mode requires the generated BAML client. Run `baml-cli generate` from the repository root and build with `--features llm`.",
            )
        }
    }

    impl Error for MissingBamlClient {}

    pub mod types {
        #[derive(Debug, Clone)]
        pub enum Union2KassistantOrKuser {
            Kassistant,
            Kuser,
        }

        #[derive(Debug, Clone)]
        pub struct PlanChatTurn {
            pub role: Union2KassistantOrKuser,
            pub content: String,
        }
    }

    pub mod sync_client {
        use super::types::PlanChatTurn;
        use super::{ClientRegistry, MissingBamlClient};

        #[derive(Debug, Clone)]
        pub struct TranslatePlanOutput {
            pub text: String,
            pub reasoning: String,
        }

        pub struct TranslatePlanClient;

        pub struct TranslatePlanCall<'a> {
            _registry: &'a ClientRegistry,
        }

        impl TranslatePlanClient {
            pub fn with_client_registry<'a>(
                &'a self,
                registry: &'a ClientRegistry,
            ) -> TranslatePlanCall<'a> {
                TranslatePlanCall {
                    _registry: registry,
                }
            }
        }

        impl TranslatePlanCall<'_> {
            pub fn call(
                &self,
                _messages: &[PlanChatTurn],
            ) -> Result<TranslatePlanOutput, MissingBamlClient> {
                Err(MissingBamlClient)
            }
        }

        #[allow(non_snake_case)]
        pub struct BRoot {
            pub TranslatePlan: TranslatePlanClient,
        }

        pub static B: BRoot = BRoot {
            TranslatePlan: TranslatePlanClient,
        };
    }
}
