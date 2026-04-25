//! Serializable **Code Mode** `Plan` contract.
//!
//! Agents author one program-shaped `Plan` in TypeScript; hosts deserialize that JSON into these
//! Rust types, validate the DAG, then expose only dry-run / execution results to the agent.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

macro_rules! plan_string_atom {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, String> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(format!("{} must be non-empty", stringify!($name)));
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

plan_string_atom! {
    /// Validated local Plan node id.
    PlanNodeId
}

plan_string_atom! {
    /// Reference to a materialized Plan node.
    NodeRef
}

plan_string_atom! {
    /// Symbolic callback/item binding name.
    BindingName
}

plan_string_atom! {
    /// Named Plan return or synthetic result field.
    OutputName
}

plan_string_atom! {
    /// Alias under which a materialized dependency is available during derived evaluation.
    InputAlias
}

/// Dotted field path after validation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FieldPath(Vec<String>);

impl FieldPath {
    pub fn new(segments: Vec<String>) -> Result<Self, String> {
        if segments.is_empty() || segments.iter().any(|s| s.trim().is_empty()) {
            return Err("FieldPath must contain non-empty segments".to_string());
        }
        Ok(Self(segments))
    }

    pub fn from_dotted(path: &str) -> Result<Self, String> {
        Self::new(path.split('.').map(str::to_string).collect())
    }

    pub fn segments(&self) -> &[String] {
        &self.0
    }

    pub fn dotted(&self) -> String {
        self.0.join(".")
    }
}

/// Typed source reference used by validated compute and derive nodes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SourceRef(pub NodeRef);

/// Effect classification (mirrors CGS capability + action output semantics; host authority).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectClass {
    Read,
    Write,
    SideEffect,
    ArtifactRead,
}

/// Expected host result shape for dry-run / planning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultShape {
    List,
    Single,
    MutationResult,
    SideEffectAck,
    Page,
    Artifact,
}

/// Qualified catalog entity key for dispatch (matches federation doctrine).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualifiedEntityKey {
    pub entry_id: String,
    pub entity: String,
}

/// Reference to a prior node for symbolic `uses_result` edges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanResultUse {
    /// Node `id` (sandbox-local string).
    pub node: String,
    /// Local binding name.
    pub r#as: String,
}

/// Cardinality contract for a data input consumed by a derived node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputCardinality {
    /// Host may broadcast only when the dependency is statically provable as singleton.
    Auto,
    /// The author explicitly requested singleton broadcast; runtime still verifies one row.
    Singleton,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardinalityAnalysis {
    StaticSingleton,
    PluralOrUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputCardinalityProof {
    StaticSingleton,
    RuntimeCheckedSingleton,
}

fn default_input_cardinality() -> InputCardinality {
    InputCardinality::Auto
}

/// Explicit dataflow input for derived Plan nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanDataInput {
    pub node: String,
    pub alias: String,
    #[serde(default = "default_input_cardinality")]
    pub cardinality: InputCardinality,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedPlanDataInput {
    pub node: PlanNodeId,
    pub alias: InputAlias,
    pub proof: InputCardinalityProof,
}

/// A structured predicate preserved alongside the rendered Plasm expression.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanPredicate {
    /// Field or dotted path segments (`updated_at`, `owner.login`, ...).
    pub field_path: Vec<String>,
    pub op: PlanPredicateOp,
    pub value: PlanValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanPredicateOp {
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
    Contains,
    In,
    Exists,
}

/// Predicate/template values in the Plan DAG. `helper` preserves intent such as `daysAgo(30)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlanValue {
    Literal {
        value: serde_json::Value,
    },
    Helper {
        name: String,
        #[serde(default)]
        args: Vec<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display: Option<String>,
    },
    BindingSymbol {
        binding: String,
        #[serde(default)]
        path: Vec<String>,
    },
    NodeSymbol {
        node: String,
        alias: String,
        #[serde(default)]
        path: Vec<String>,
    },
    Symbol {
        path: String,
    },
    Template {
        template: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        input_bindings: Vec<PlanInputBinding>,
    },
    Array {
        #[serde(default)]
        items: Vec<PlanValue>,
    },
    Object {
        #[serde(default)]
        fields: BTreeMap<String, PlanValue>,
    },
}

/// Root `Plan` kind. We accept omission on the wire, but the canonical form is always a program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanKind {
    Program,
}

fn default_plan_kind() -> PlanKind {
    PlanKind::Program
}

fn default_plan_version() -> u32 {
    1
}

/// Single code-mode artifact: a program-shaped Plan DAG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    #[serde(default = "default_plan_version")]
    pub version: u32,
    #[serde(default = "default_plan_kind")]
    pub kind: PlanKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub nodes: Vec<PlanNode>,
    #[serde(rename = "return")]
    pub return_value: PlanReturn,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

pub type RawPlanArtifact = Plan;

/// Agent-visible return shape: a single node, a parallel set, or named outputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PlanReturn {
    Parallel { parallel: Vec<String> },
    Record(BTreeMap<String, String>),
    Node(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidatedPlanReturn {
    Parallel { parallel: Vec<NodeRef> },
    Record(BTreeMap<OutputName, NodeRef>),
    Node(NodeRef),
}

impl ValidatedPlanReturn {
    pub fn refs(&self) -> Vec<&NodeRef> {
        match self {
            ValidatedPlanReturn::Node(id) => vec![id],
            ValidatedPlanReturn::Parallel { parallel } => parallel.iter().collect(),
            ValidatedPlanReturn::Record(map) => map.values().collect(),
        }
    }
}

/// Proof-bearing Plan artifact consumed by dry-run and execution.
#[derive(Debug, Clone)]
pub struct ValidatedPlanArtifact {
    pub version: u32,
    pub name: Option<String>,
    pub metadata: BTreeMap<String, serde_json::Value>,
    pub nodes: Vec<ValidatedPlanNode>,
    pub topo: Vec<PlanNodeId>,
    pub return_value: ValidatedPlanReturn,
    pub node_indices: HashMap<PlanNodeId, usize>,
    pub approval_gates: Vec<PlanNodeId>,
}

pub type ValidatedPlan = ValidatedPlanArtifact;

#[derive(Debug, Clone)]
pub enum ValidatedPlanNode {
    Surface(ValidatedSurfaceNode),
    Data(ValidatedDataNode),
    Derive(ValidatedDeriveNode),
    Compute(ValidatedComputeNode),
    ForEach(ValidatedForEachNode),
}

#[derive(Debug, Clone)]
pub struct ValidatedSurfaceNode {
    pub id: PlanNodeId,
    pub kind: PlanNodeKind,
    pub qualified_entity: Option<QualifiedEntityKey>,
    pub expr: String,
    pub effect_class: EffectClass,
    pub result_shape: ResultShape,
    pub projection: Vec<String>,
    pub predicates: Vec<PlanPredicate>,
    pub depends_on: Vec<PlanNodeId>,
    pub uses_result: Vec<PlanResultUse>,
    pub approval: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ValidatedDataNode {
    pub id: PlanNodeId,
    pub effect_class: EffectClass,
    pub result_shape: ResultShape,
    pub data: PlanValue,
    pub depends_on: Vec<PlanNodeId>,
    pub uses_result: Vec<PlanResultUse>,
}

#[derive(Debug, Clone)]
pub struct ValidatedDeriveNode {
    pub id: PlanNodeId,
    pub effect_class: EffectClass,
    pub result_shape: ResultShape,
    pub source: PlanNodeId,
    pub item_binding: BindingName,
    pub inputs: Vec<ValidatedPlanDataInput>,
    pub value: PlanValue,
    pub depends_on: Vec<PlanNodeId>,
    pub uses_result: Vec<PlanResultUse>,
}

#[derive(Debug, Clone)]
pub struct ValidatedComputeNode {
    pub id: PlanNodeId,
    pub effect_class: EffectClass,
    pub result_shape: ResultShape,
    pub compute: ComputeTemplate,
    pub depends_on: Vec<PlanNodeId>,
    pub uses_result: Vec<PlanResultUse>,
}

#[derive(Debug, Clone)]
pub struct ValidatedForEachNode {
    pub id: PlanNodeId,
    pub effect_class: EffectClass,
    pub result_shape: ResultShape,
    pub source: PlanNodeId,
    pub item_binding: BindingName,
    pub effect_template: EffectTemplate,
    pub projection: Vec<String>,
    pub predicates: Vec<PlanPredicate>,
    pub depends_on: Vec<PlanNodeId>,
    pub uses_result: Vec<PlanResultUse>,
    pub approval: Option<String>,
}

impl ValidatedPlanNode {
    pub fn id(&self) -> &PlanNodeId {
        match self {
            Self::Surface(n) => &n.id,
            Self::Data(n) => &n.id,
            Self::Derive(n) => &n.id,
            Self::Compute(n) => &n.id,
            Self::ForEach(n) => &n.id,
        }
    }

    pub fn kind(&self) -> PlanNodeKind {
        match self {
            Self::Surface(n) => n.kind,
            Self::Data(_) => PlanNodeKind::Data,
            Self::Derive(_) => PlanNodeKind::Derive,
            Self::Compute(_) => PlanNodeKind::Compute,
            Self::ForEach(_) => PlanNodeKind::ForEach,
        }
    }

    pub fn effect_class(&self) -> EffectClass {
        match self {
            Self::Surface(n) => n.effect_class,
            Self::Data(n) => n.effect_class,
            Self::Derive(n) => n.effect_class,
            Self::Compute(n) => n.effect_class,
            Self::ForEach(n) => n.effect_class,
        }
    }

    pub fn result_shape(&self) -> ResultShape {
        match self {
            Self::Surface(n) => n.result_shape,
            Self::Data(n) => n.result_shape,
            Self::Derive(n) => n.result_shape,
            Self::Compute(n) => n.result_shape,
            Self::ForEach(n) => n.result_shape,
        }
    }

    pub fn depends_on(&self) -> &[PlanNodeId] {
        match self {
            Self::Surface(n) => &n.depends_on,
            Self::Data(n) => &n.depends_on,
            Self::Derive(n) => &n.depends_on,
            Self::Compute(n) => &n.depends_on,
            Self::ForEach(n) => &n.depends_on,
        }
    }

    pub fn uses_result(&self) -> &[PlanResultUse] {
        match self {
            Self::Surface(n) => &n.uses_result,
            Self::Data(n) => &n.uses_result,
            Self::Derive(n) => &n.uses_result,
            Self::Compute(n) => &n.uses_result,
            Self::ForEach(n) => &n.uses_result,
        }
    }

    pub fn to_plan_node(&self) -> PlanNode {
        match self {
            Self::Surface(n) => PlanNode {
                id: n.id.as_str().to_string(),
                kind: n.kind,
                qualified_entity: n.qualified_entity.clone(),
                expr: Some(n.expr.clone()),
                effect_class: n.effect_class,
                result_shape: n.result_shape,
                projection: n.projection.clone(),
                predicates: n.predicates.clone(),
                source: None,
                item_binding: None,
                effect_template: None,
                approval: n.approval.clone(),
                data: None,
                derive_template: None,
                compute: None,
                depends_on: n
                    .depends_on
                    .iter()
                    .map(|id| id.as_str().to_string())
                    .collect(),
                uses_result: n.uses_result.clone(),
            },
            Self::Data(n) => PlanNode {
                id: n.id.as_str().to_string(),
                kind: PlanNodeKind::Data,
                qualified_entity: None,
                expr: None,
                effect_class: n.effect_class,
                result_shape: n.result_shape,
                projection: vec![],
                predicates: vec![],
                source: None,
                item_binding: None,
                effect_template: None,
                approval: None,
                data: Some(n.data.clone()),
                derive_template: None,
                compute: None,
                depends_on: n
                    .depends_on
                    .iter()
                    .map(|id| id.as_str().to_string())
                    .collect(),
                uses_result: n.uses_result.clone(),
            },
            Self::Derive(n) => PlanNode {
                id: n.id.as_str().to_string(),
                kind: PlanNodeKind::Derive,
                qualified_entity: None,
                expr: None,
                effect_class: n.effect_class,
                result_shape: n.result_shape,
                projection: vec![],
                predicates: vec![],
                source: None,
                item_binding: None,
                effect_template: None,
                approval: None,
                data: None,
                derive_template: Some(DeriveTemplate {
                    kind: DeriveKind::Map,
                    source: Some(n.source.as_str().to_string()),
                    item_binding: Some(n.item_binding.as_str().to_string()),
                    inputs: n.inputs.iter().map(PlanDataInput::from).collect(),
                    value: n.value.clone(),
                }),
                compute: None,
                depends_on: n
                    .depends_on
                    .iter()
                    .map(|id| id.as_str().to_string())
                    .collect(),
                uses_result: n.uses_result.clone(),
            },
            Self::Compute(n) => PlanNode {
                id: n.id.as_str().to_string(),
                kind: PlanNodeKind::Compute,
                qualified_entity: None,
                expr: None,
                effect_class: n.effect_class,
                result_shape: n.result_shape,
                projection: vec![],
                predicates: vec![],
                source: None,
                item_binding: None,
                effect_template: None,
                approval: None,
                data: None,
                derive_template: None,
                compute: Some(n.compute.clone()),
                depends_on: n
                    .depends_on
                    .iter()
                    .map(|id| id.as_str().to_string())
                    .collect(),
                uses_result: n.uses_result.clone(),
            },
            Self::ForEach(n) => PlanNode {
                id: n.id.as_str().to_string(),
                kind: PlanNodeKind::ForEach,
                qualified_entity: None,
                expr: None,
                effect_class: n.effect_class,
                result_shape: n.result_shape,
                projection: n.projection.clone(),
                predicates: n.predicates.clone(),
                source: Some(n.source.as_str().to_string()),
                item_binding: Some(n.item_binding.as_str().to_string()),
                effect_template: Some(n.effect_template.clone()),
                approval: n.approval.clone(),
                data: None,
                derive_template: None,
                compute: None,
                depends_on: n
                    .depends_on
                    .iter()
                    .map(|id| id.as_str().to_string())
                    .collect(),
                uses_result: n.uses_result.clone(),
            },
        }
    }
}

impl From<&ValidatedPlanDataInput> for PlanDataInput {
    fn from(input: &ValidatedPlanDataInput) -> Self {
        Self {
            node: input.node.as_str().to_string(),
            alias: input.alias.as_str().to_string(),
            cardinality: match input.proof {
                InputCardinalityProof::StaticSingleton => InputCardinality::Auto,
                InputCardinalityProof::RuntimeCheckedSingleton => InputCardinality::Singleton,
            },
        }
    }
}

impl ValidatedPlanArtifact {
    pub fn to_raw_plan(&self) -> RawPlanArtifact {
        RawPlanArtifact {
            version: self.version,
            kind: PlanKind::Program,
            name: self.name.clone(),
            nodes: self
                .nodes
                .iter()
                .map(ValidatedPlanNode::to_plan_node)
                .collect(),
            return_value: match &self.return_value {
                ValidatedPlanReturn::Node(id) => PlanReturn::Node(id.as_str().to_string()),
                ValidatedPlanReturn::Parallel { parallel } => PlanReturn::Parallel {
                    parallel: parallel.iter().map(|id| id.as_str().to_string()).collect(),
                },
                ValidatedPlanReturn::Record(record) => PlanReturn::Record(
                    record
                        .iter()
                        .map(|(name, id)| (name.as_str().to_string(), id.as_str().to_string()))
                        .collect(),
                ),
            },
            metadata: self.metadata.clone(),
        }
    }
}

/// Kinds the typed Plan DAG understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanNodeKind {
    Query,
    Search,
    Get,
    Create,
    Update,
    Delete,
    Action,
    Data,
    Derive,
    Compute,
    ForEach,
}

impl PlanNodeKind {
    pub fn has_surface_expr(self) -> bool {
        matches!(
            self,
            PlanNodeKind::Query
                | PlanNodeKind::Search
                | PlanNodeKind::Get
                | PlanNodeKind::Create
                | PlanNodeKind::Update
                | PlanNodeKind::Delete
                | PlanNodeKind::Action
        )
    }

    pub fn is_template_allowed(self) -> bool {
        matches!(
            self,
            PlanNodeKind::Query
                | PlanNodeKind::Search
                | PlanNodeKind::Get
                | PlanNodeKind::Create
                | PlanNodeKind::Update
                | PlanNodeKind::Delete
                | PlanNodeKind::Action
        )
    }
}

/// One typed node in the Plan DAG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanNode {
    pub id: String,
    pub kind: PlanNodeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualified_entity: Option<QualifiedEntityKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expr: Option<String>,
    pub effect_class: EffectClass,
    pub result_shape: ResultShape,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projection: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub predicates: Vec<PlanPredicate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_binding: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_template: Option<EffectTemplate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<PlanValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derive_template: Option<DeriveTemplate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compute: Option<ComputeTemplate>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub uses_result: Vec<PlanResultUse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectTemplate {
    pub kind: PlanNodeKind,
    pub qualified_entity: QualifiedEntityKey,
    pub expr_template: String,
    pub effect_class: EffectClass,
    pub result_shape: ResultShape,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projection: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_bindings: Vec<PlanInputBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanInputBinding {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeriveTemplate {
    pub kind: DeriveKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_binding: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<PlanDataInput>,
    pub value: PlanValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeriveKind {
    Map,
    Data,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputeTemplate {
    pub source: String,
    pub op: ComputeOp,
    pub schema: SyntheticResultSchema,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_size: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ComputeOp {
    Project {
        fields: BTreeMap<OutputName, FieldPath>,
    },
    Filter {
        predicates: Vec<PlanPredicate>,
    },
    GroupBy {
        key: FieldPath,
        aggregates: Vec<AggregateSpec>,
    },
    Aggregate {
        aggregates: Vec<AggregateSpec>,
    },
    Sort {
        key: FieldPath,
        #[serde(default)]
        descending: bool,
    },
    Limit {
        count: usize,
    },
    TableFromMatrix {
        columns: Vec<OutputName>,
        #[serde(default)]
        has_header: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregateSpec {
    pub name: OutputName,
    pub function: AggregateFunction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<FieldPath>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregateFunction {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntheticResultSchema {
    #[serde(default)]
    pub entity: Option<String>,
    pub fields: Vec<SyntheticFieldSchema>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntheticFieldSchema {
    pub name: OutputName,
    pub value_kind: SyntheticValueKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<FieldPath>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyntheticValueKind {
    Null,
    Boolean,
    Integer,
    Number,
    String,
    Array,
    Object,
    Unknown,
}

fn has_cycle(adj: &[Vec<usize>]) -> bool {
    let n = adj.len();
    let mut vis = vec![0u8; n];
    fn dfs(u: usize, adj: &[Vec<usize>], vis: &mut [u8]) -> bool {
        vis[u] = 1;
        for &v in &adj[u] {
            if v >= vis.len() {
                continue;
            }
            if vis[v] == 1 {
                return true;
            }
            if vis[v] == 0 && dfs(v, adj, vis) {
                return true;
            }
        }
        vis[u] = 2;
        false
    }
    for i in 0..n {
        if vis[i] == 0 && dfs(i, adj, &mut vis) {
            return true;
        }
    }
    false
}

fn return_refs(ret: &PlanReturn) -> Vec<&str> {
    match ret {
        PlanReturn::Node(id) => vec![id.as_str()],
        PlanReturn::Parallel { parallel } => parallel.iter().map(String::as_str).collect(),
        PlanReturn::Record(map) => map.values().map(String::as_str).collect(),
    }
}

/// Parse and normalize Plan JSON. The canonical shape is program-DAG-only; a narrow legacy
/// `{ version, nodes: [{ expr }] }` shim is accepted and normalized to a program Plan.
pub fn parse_plan_value(plan: &serde_json::Value) -> Result<Plan, String> {
    if is_legacy_expr_list(plan) {
        return normalize_legacy_expr_list(plan);
    }
    serde_json::from_value(plan.clone()).map_err(|e| format!("Plan JSON: {e}"))
}

fn is_legacy_expr_list(plan: &serde_json::Value) -> bool {
    let Some(obj) = plan.as_object() else {
        return false;
    };
    obj.contains_key("nodes")
        && !obj.contains_key("return")
        && !obj.contains_key("kind")
        && !obj.contains_key("name")
}

fn normalize_legacy_expr_list(plan: &serde_json::Value) -> Result<Plan, String> {
    let obj = plan
        .as_object()
        .ok_or_else(|| "plan must be a JSON object".to_string())?;
    let version = obj
        .get("version")
        .and_then(|v| v.as_u64())
        .unwrap_or(1)
        .try_into()
        .map_err(|_| "plan.version must fit in u32".to_string())?;
    let nodes = obj
        .get("nodes")
        .and_then(|n| n.as_array())
        .ok_or_else(|| "plan.nodes must be a JSON array".to_string())?;
    let mut out = Vec::new();
    for (i, n) in nodes.iter().enumerate() {
        let o = n
            .as_object()
            .ok_or_else(|| format!("plan.nodes[{i}] must be a JSON object"))?;
        let expr = o
            .get("expr")
            .and_then(|e| e.as_str())
            .ok_or_else(|| format!("plan.nodes[{i}].expr must be a string"))?
            .trim();
        if expr.is_empty() {
            return Err(format!("plan.nodes[{i}].expr is empty"));
        }
        let id = o
            .get("id")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("n{}", i + 1));
        out.push(PlanNode {
            id,
            kind: PlanNodeKind::Query,
            qualified_entity: None,
            expr: Some(expr.to_string()),
            effect_class: EffectClass::Read,
            result_shape: ResultShape::List,
            projection: vec![],
            predicates: vec![],
            source: None,
            item_binding: None,
            effect_template: None,
            approval: None,
            data: None,
            derive_template: None,
            compute: None,
            depends_on: vec![],
            uses_result: vec![],
        });
    }
    let first = out
        .first()
        .map(|n| n.id.clone())
        .ok_or_else(|| "plan.nodes must be non-empty".to_string())?;
    let mut metadata = BTreeMap::new();
    metadata.insert("legacy_expr_list".to_string(), serde_json::json!(true));
    Ok(Plan {
        version,
        kind: PlanKind::Program,
        name: Some("legacy-expression-list".to_string()),
        nodes: out,
        return_value: PlanReturn::Node(first),
        metadata,
    })
}

/// Parse and validate one program-shaped Plan.
pub fn validate_plan(plan: &Plan) -> Result<(), String> {
    validate_plan_artifact(plan).map(|_| ())
}

/// Parse and validate one program-shaped Plan, returning typed execution metadata.
pub fn validate_plan_artifact(plan: &Plan) -> Result<ValidatedPlan, String> {
    if plan.version != 1 {
        return Err(format!("unsupported Plan version: {}", plan.version));
    }
    if plan.nodes.is_empty() {
        return Err("plan.nodes must be non-empty".to_string());
    }
    let mut by_id: HashMap<String, usize> = HashMap::new();
    for (i, n) in plan.nodes.iter().enumerate() {
        if n.id.trim().is_empty() {
            return Err(format!("plan.nodes[{i}].id is empty"));
        }
        if by_id.insert(n.id.clone(), i).is_some() {
            return Err(format!("duplicate plan node id: {}", n.id));
        }
        PlanNodeId::new(n.id.clone()).map_err(|e| format!("plan.nodes[{i}].id: {e}"))?;
    }

    for (i, n) in plan.nodes.iter().enumerate() {
        if n.kind.has_surface_expr() {
            let expr = n
                .expr
                .as_ref()
                .ok_or_else(|| format!("plan.nodes[{i}].expr is required for {:?}", n.kind))?;
            if expr.trim().is_empty() {
                return Err(format!("plan.nodes[{i}].expr is empty"));
            }
            if n.qualified_entity.is_none()
                && !plan
                    .metadata
                    .get("legacy_expr_list")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            {
                return Err(format!(
                    "plan.nodes[{i}].qualified_entity is required for executable node {:?}",
                    n.kind
                ));
            }
        }
        if n.kind == PlanNodeKind::Derive && n.expr.is_some() {
            return Err(format!("plan.nodes[{i}].derive must not carry expr"));
        }
        if n.kind == PlanNodeKind::Data {
            if n.data.is_none() {
                return Err(format!("plan.nodes[{i}].data is required for data nodes"));
            }
            if n.expr.is_some() {
                return Err(format!("plan.nodes[{i}].data must not carry expr"));
            }
        }
        if n.kind == PlanNodeKind::Compute {
            let compute = n
                .compute
                .as_ref()
                .ok_or_else(|| format!("plan.nodes[{i}].compute is required for compute nodes"))?;
            if n.expr.is_some() || n.data.is_some() || n.effect_template.is_some() {
                return Err(format!(
                    "plan.nodes[{i}].compute must not carry expr, data, or effect_template"
                ));
            }
            validate_compute_template(compute, i, &by_id)?;
        } else if n.compute.is_some() {
            return Err(format!(
                "plan.nodes[{i}].compute is only valid for compute nodes"
            ));
        }
        if let Some(data) = &n.data {
            validate_plan_value_expr(data, i, "data")?;
        }
        if let Some(derive_template) = &n.derive_template {
            validate_plan_value_expr(&derive_template.value, i, "derive_template.value")?;
            if derive_template.kind == DeriveKind::Map {
                let source = derive_template.source.as_deref().unwrap_or_default();
                if source.trim().is_empty() || !by_id.contains_key(source) {
                    return Err(format!(
                        "plan.nodes[{i}].derive_template.source references unknown id {source:?}"
                    ));
                }
                let binding = derive_template.item_binding.as_deref().unwrap_or_default();
                if binding.trim().is_empty() {
                    return Err(format!(
                        "plan.nodes[{i}].derive_template.item_binding is required for map"
                    ));
                }
            }
            for input in &derive_template.inputs {
                validate_plan_data_input(input, i, &by_id)?;
                if input.cardinality == InputCardinality::Auto
                    && analyze_static_cardinality(plan, &by_id, input.node.as_str())
                        != CardinalityAnalysis::StaticSingleton
                {
                    return Err(format!(
                        "plan.nodes[{i}].derive_template.inputs node {:?} is not statically singleton; wrap it with Plan.singleton(...) to request runtime-checked broadcast",
                        input.node
                    ));
                }
            }
            validate_derive_value_inputs(derive_template, i)?;
        }
        for (j, p) in n.predicates.iter().enumerate() {
            validate_predicate(p, i, j)?;
        }
        if n.kind == PlanNodeKind::ForEach {
            let source = n
                .source
                .as_ref()
                .ok_or_else(|| format!("plan.nodes[{i}].source is required for for_each"))?;
            if !by_id.contains_key(source) {
                return Err(format!(
                    "plan.nodes[{i}].source references unknown id {source:?}"
                ));
            }
            let binding = n.item_binding.as_deref().unwrap_or_default();
            if binding.trim().is_empty() {
                return Err(format!(
                    "plan.nodes[{i}].item_binding is required for for_each"
                ));
            }
            let template = n
                .effect_template
                .as_ref()
                .ok_or_else(|| format!("plan.nodes[{i}].effect_template is required"))?;
            validate_effect_template(template, i)?;
            for b in &template.input_bindings {
                if !b.from.starts_with(&format!("{binding}."))
                    && b.from.as_str() != binding
                    && !b.from.contains('.')
                {
                    return Err(format!(
                        "plan.nodes[{i}].effect_template.input_bindings source {:?} does not reference item binding {:?}",
                        b.from, binding
                    ));
                }
            }
        }
    }

    let mut adj: Vec<Vec<usize>> = vec![vec![]; plan.nodes.len()];
    for (i, n) in plan.nodes.iter().enumerate() {
        for d in &n.depends_on {
            let t = *by_id
                .get(d)
                .ok_or_else(|| format!("plan.nodes[{i}].depends_on references unknown id {d:?}"))?;
            adj[i].push(t);
        }
        for u in &n.uses_result {
            let t = *by_id.get(&u.node).ok_or_else(|| {
                format!(
                    "plan.nodes[{i}].uses_result.node {:?} is not a known id",
                    u.node
                )
            })?;
            if !adj[i].contains(&t) {
                adj[i].push(t);
            }
        }
        if let Some(source) = &n.source {
            let t = *by_id.get(source).ok_or_else(|| {
                format!("plan.nodes[{i}].source references unknown id {source:?}")
            })?;
            if !adj[i].contains(&t) {
                adj[i].push(t);
            }
        }
        if let Some(derive_template) = &n.derive_template {
            if let Some(source) = &derive_template.source {
                let t = *by_id.get(source).ok_or_else(|| {
                    format!(
                        "plan.nodes[{i}].derive_template.source references unknown id {source:?}"
                    )
                })?;
                if !adj[i].contains(&t) {
                    adj[i].push(t);
                }
            }
            for input in &derive_template.inputs {
                let t = *by_id.get(&input.node).ok_or_else(|| {
                    format!(
                        "plan.nodes[{i}].derive_template.inputs references unknown id {:?}",
                        input.node
                    )
                })?;
                if !adj[i].contains(&t) {
                    adj[i].push(t);
                }
            }
        }
        if let Some(compute) = &n.compute {
            let t = *by_id.get(&compute.source).ok_or_else(|| {
                format!(
                    "plan.nodes[{i}].compute.source references unknown id {:?}",
                    compute.source
                )
            })?;
            if !adj[i].contains(&t) {
                adj[i].push(t);
            }
        }
    }
    if has_cycle(&adj) {
        return Err("plan: depends_on has a cycle".to_string());
    }
    let return_value = validate_return_refs(&plan.return_value, &by_id)?;
    let topo = topological_order(plan, &adj)?;
    let mut node_indices = HashMap::new();
    for (id, idx) in &by_id {
        node_indices.insert(PlanNodeId::new(id.clone())?, *idx);
    }
    let nodes = plan
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| validated_node_from_raw(plan, node, i, &by_id))
        .collect::<Result<Vec<_>, _>>()?;
    let approval_gates = plan
        .nodes
        .iter()
        .filter(|n| {
            matches!(
                n.kind,
                PlanNodeKind::Create
                    | PlanNodeKind::Update
                    | PlanNodeKind::Delete
                    | PlanNodeKind::Action
                    | PlanNodeKind::ForEach
            ) || matches!(n.effect_class, EffectClass::Write | EffectClass::SideEffect)
        })
        .map(|n| PlanNodeId::new(n.id.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ValidatedPlan {
        version: plan.version,
        name: plan.name.clone(),
        metadata: plan.metadata.clone(),
        nodes,
        topo,
        return_value,
        node_indices,
        approval_gates,
    })
}

fn validated_node_from_raw(
    plan: &Plan,
    node: &PlanNode,
    node_index: usize,
    by_id: &HashMap<String, usize>,
) -> Result<ValidatedPlanNode, String> {
    let id = PlanNodeId::new(node.id.clone())?;
    let depends_on = typed_node_ids(&node.depends_on)?;
    let uses_result = node.uses_result.clone();
    match node.kind {
        kind @ (PlanNodeKind::Query
        | PlanNodeKind::Search
        | PlanNodeKind::Get
        | PlanNodeKind::Create
        | PlanNodeKind::Update
        | PlanNodeKind::Delete
        | PlanNodeKind::Action) => Ok(ValidatedPlanNode::Surface(ValidatedSurfaceNode {
            id,
            kind,
            qualified_entity: node.qualified_entity.clone(),
            expr: node
                .expr
                .clone()
                .ok_or_else(|| format!("plan.nodes[{node_index}].expr is required"))?,
            effect_class: node.effect_class,
            result_shape: node.result_shape,
            projection: node.projection.clone(),
            predicates: node.predicates.clone(),
            depends_on,
            uses_result,
            approval: node.approval.clone(),
        })),
        PlanNodeKind::Data => Ok(ValidatedPlanNode::Data(ValidatedDataNode {
            id,
            effect_class: node.effect_class,
            result_shape: node.result_shape,
            data: node
                .data
                .clone()
                .ok_or_else(|| format!("plan.nodes[{node_index}].data is required"))?,
            depends_on,
            uses_result,
        })),
        PlanNodeKind::Derive => {
            let template = node
                .derive_template
                .as_ref()
                .ok_or_else(|| format!("plan.nodes[{node_index}].derive_template is required"))?;
            let source = template
                .source
                .as_ref()
                .ok_or_else(|| {
                    format!("plan.nodes[{node_index}].derive_template.source is required")
                })
                .and_then(|s| PlanNodeId::new(s.clone()))?;
            let item_binding = template
                .item_binding
                .as_ref()
                .ok_or_else(|| {
                    format!("plan.nodes[{node_index}].derive_template.item_binding is required")
                })
                .and_then(|s| BindingName::new(s.clone()))?;
            let inputs = template
                .inputs
                .iter()
                .map(|input| validated_data_input(plan, input, by_id))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ValidatedPlanNode::Derive(ValidatedDeriveNode {
                id,
                effect_class: node.effect_class,
                result_shape: node.result_shape,
                source,
                item_binding,
                inputs,
                value: template.value.clone(),
                depends_on,
                uses_result,
            }))
        }
        PlanNodeKind::Compute => Ok(ValidatedPlanNode::Compute(ValidatedComputeNode {
            id,
            effect_class: node.effect_class,
            result_shape: node.result_shape,
            compute: node
                .compute
                .clone()
                .ok_or_else(|| format!("plan.nodes[{node_index}].compute is required"))?,
            depends_on,
            uses_result,
        })),
        PlanNodeKind::ForEach => Ok(ValidatedPlanNode::ForEach(ValidatedForEachNode {
            id,
            effect_class: node.effect_class,
            result_shape: node.result_shape,
            source: node
                .source
                .as_ref()
                .ok_or_else(|| format!("plan.nodes[{node_index}].source is required"))
                .and_then(|s| PlanNodeId::new(s.clone()))?,
            item_binding: node
                .item_binding
                .as_ref()
                .ok_or_else(|| format!("plan.nodes[{node_index}].item_binding is required"))
                .and_then(|s| BindingName::new(s.clone()))?,
            effect_template: node
                .effect_template
                .clone()
                .ok_or_else(|| format!("plan.nodes[{node_index}].effect_template is required"))?,
            projection: node.projection.clone(),
            predicates: node.predicates.clone(),
            depends_on,
            uses_result,
            approval: node.approval.clone(),
        })),
    }
}

fn typed_node_ids(raw: &[String]) -> Result<Vec<PlanNodeId>, String> {
    raw.iter().cloned().map(PlanNodeId::new).collect()
}

fn validated_data_input(
    plan: &Plan,
    input: &PlanDataInput,
    by_id: &HashMap<String, usize>,
) -> Result<ValidatedPlanDataInput, String> {
    let proof = match input.cardinality {
        InputCardinality::Singleton => InputCardinalityProof::RuntimeCheckedSingleton,
        InputCardinality::Auto => {
            match analyze_static_cardinality(plan, by_id, input.node.as_str()) {
                CardinalityAnalysis::StaticSingleton => InputCardinalityProof::StaticSingleton,
                CardinalityAnalysis::PluralOrUnknown => {
                    return Err(format!(
                        "input {:?} is not statically singleton and lacks explicit singleton proof",
                        input.node
                    ))
                }
            }
        }
    };
    Ok(ValidatedPlanDataInput {
        node: PlanNodeId::new(input.node.clone())?,
        alias: InputAlias::new(input.alias.clone())?,
        proof,
    })
}

fn validate_effect_template(t: &EffectTemplate, node_index: usize) -> Result<(), String> {
    if !t.kind.is_template_allowed() {
        return Err(format!(
            "plan.nodes[{node_index}].effect_template.kind {:?} is not executable",
            t.kind
        ));
    }
    if t.expr_template.trim().is_empty() {
        return Err(format!(
            "plan.nodes[{node_index}].effect_template.expr_template is empty"
        ));
    }
    for b in &t.input_bindings {
        if b.from.trim().is_empty() || b.to.trim().is_empty() {
            return Err(format!(
                "plan.nodes[{node_index}].effect_template.input_bindings must be non-empty"
            ));
        }
    }
    Ok(())
}

fn validate_plan_data_input(
    input: &PlanDataInput,
    node_index: usize,
    by_id: &HashMap<String, usize>,
) -> Result<(), String> {
    if input.node.trim().is_empty() || !by_id.contains_key(&input.node) {
        return Err(format!(
            "plan.nodes[{node_index}].derive_template.inputs references unknown id {:?}",
            input.node
        ));
    }
    if input.alias.trim().is_empty() {
        return Err(format!(
            "plan.nodes[{node_index}].derive_template.inputs alias must be non-empty"
        ));
    }
    Ok(())
}

fn validate_derive_value_inputs(
    template: &DeriveTemplate,
    node_index: usize,
) -> Result<(), String> {
    let mut inputs_by_alias = HashMap::new();
    for input in &template.inputs {
        if inputs_by_alias
            .insert(input.alias.as_str(), input.node.as_str())
            .is_some()
        {
            return Err(format!(
                "plan.nodes[{node_index}].derive_template.inputs duplicate alias {:?}",
                input.alias
            ));
        }
    }
    validate_plan_value_input_refs(&template.value, node_index, &inputs_by_alias)
}

fn validate_plan_value_input_refs(
    value: &PlanValue,
    node_index: usize,
    inputs_by_alias: &HashMap<&str, &str>,
) -> Result<(), String> {
    match value {
        PlanValue::NodeSymbol { node, alias, .. } => match inputs_by_alias.get(alias.as_str()) {
            Some(input_node) if *input_node == node.as_str() => Ok(()),
            Some(input_node) => Err(format!(
                "plan.nodes[{node_index}].derive_template.value node symbol alias {:?} points at {:?}, not {:?}",
                alias, input_node, node
            )),
            None => Err(format!(
                "plan.nodes[{node_index}].derive_template.value node symbol alias {:?} is not declared in inputs",
                alias
            )),
        },
        PlanValue::Template { input_bindings, .. } => {
            for binding in input_bindings {
                let Some((alias, _)) = binding.from.split_once('.') else {
                    continue;
                };
                if inputs_by_alias.contains_key(alias) {
                    continue;
                }
            }
            Ok(())
        }
        PlanValue::Array { items } => {
            for item in items {
                validate_plan_value_input_refs(item, node_index, inputs_by_alias)?;
            }
            Ok(())
        }
        PlanValue::Object { fields } => {
            for field in fields.values() {
                validate_plan_value_input_refs(field, node_index, inputs_by_alias)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn analyze_static_cardinality(
    plan: &Plan,
    by_id: &HashMap<String, usize>,
    node_id: &str,
) -> CardinalityAnalysis {
    fn inner(
        plan: &Plan,
        by_id: &HashMap<String, usize>,
        node_id: &str,
        memo: &mut HashMap<String, CardinalityAnalysis>,
    ) -> CardinalityAnalysis {
        if let Some(v) = memo.get(node_id) {
            return *v;
        }
        let Some(index) = by_id.get(node_id).copied() else {
            return CardinalityAnalysis::PluralOrUnknown;
        };
        let node = &plan.nodes[index];
        let singleton = match node.kind {
            PlanNodeKind::Get => CardinalityAnalysis::StaticSingleton,
            PlanNodeKind::Data => match &node.data {
                Some(PlanValue::Array { items }) if items.len() == 1 => {
                    CardinalityAnalysis::StaticSingleton
                }
                Some(PlanValue::Literal { value })
                    if value.as_array().is_none_or(|items| items.len() == 1) =>
                {
                    CardinalityAnalysis::StaticSingleton
                }
                Some(PlanValue::Array { .. } | PlanValue::Literal { .. }) | None => {
                    CardinalityAnalysis::PluralOrUnknown
                }
                Some(_) => CardinalityAnalysis::StaticSingleton,
            },
            PlanNodeKind::Derive => node
                .derive_template
                .as_ref()
                .and_then(|t| t.source.as_deref())
                .map(|source| inner(plan, by_id, source, memo))
                .unwrap_or(CardinalityAnalysis::PluralOrUnknown),
            PlanNodeKind::Compute => node
                .compute
                .as_ref()
                .map(|compute| match &compute.op {
                    ComputeOp::Aggregate { .. } => CardinalityAnalysis::StaticSingleton,
                    ComputeOp::Project { .. }
                    | ComputeOp::Filter { .. }
                    | ComputeOp::Sort { .. }
                    | ComputeOp::TableFromMatrix { .. } => {
                        inner(plan, by_id, &compute.source, memo)
                    }
                    ComputeOp::Limit { count } if *count <= 1 => {
                        CardinalityAnalysis::StaticSingleton
                    }
                    ComputeOp::Limit { .. } | ComputeOp::GroupBy { .. } => {
                        CardinalityAnalysis::PluralOrUnknown
                    }
                })
                .unwrap_or(CardinalityAnalysis::PluralOrUnknown),
            _ => CardinalityAnalysis::PluralOrUnknown,
        };
        memo.insert(node_id.to_string(), singleton);
        singleton
    }
    inner(plan, by_id, node_id, &mut HashMap::new())
}

fn validate_compute_template(
    t: &ComputeTemplate,
    node_index: usize,
    by_id: &HashMap<String, usize>,
) -> Result<(), String> {
    if t.source.trim().is_empty() || !by_id.contains_key(&t.source) {
        return Err(format!(
            "plan.nodes[{node_index}].compute.source references unknown id {:?}",
            t.source
        ));
    }
    if t.schema.fields.is_empty() {
        return Err(format!(
            "plan.nodes[{node_index}].compute.schema.fields must be non-empty"
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for field in &t.schema.fields {
        if !seen.insert(field.name.as_str().to_string()) {
            return Err(format!(
                "plan.nodes[{node_index}].compute.schema.fields contains duplicate {:?}",
                field.name.as_str()
            ));
        }
    }
    if t.page_size == Some(0) {
        return Err(format!(
            "plan.nodes[{node_index}].compute.page_size must be greater than zero"
        ));
    }
    match &t.op {
        ComputeOp::Project { fields } => {
            if fields.is_empty() {
                return Err(format!(
                    "plan.nodes[{node_index}].compute.project.fields must be non-empty"
                ));
            }
        }
        ComputeOp::Filter { predicates } => {
            for (j, p) in predicates.iter().enumerate() {
                validate_predicate(p, node_index, j)?;
            }
        }
        ComputeOp::GroupBy { aggregates, .. } | ComputeOp::Aggregate { aggregates } => {
            if aggregates.is_empty() {
                return Err(format!(
                    "plan.nodes[{node_index}].compute aggregates must be non-empty"
                ));
            }
            for agg in aggregates {
                if agg.function != AggregateFunction::Count && agg.field.is_none() {
                    return Err(format!(
                        "plan.nodes[{node_index}].compute aggregate {:?} requires a field",
                        agg.name.as_str()
                    ));
                }
            }
        }
        ComputeOp::Limit { count } if *count == 0 => {
            return Err(format!(
                "plan.nodes[{node_index}].compute.limit.count must be greater than zero"
            ));
        }
        ComputeOp::TableFromMatrix { columns, .. } if columns.is_empty() => {
            return Err(format!(
                "plan.nodes[{node_index}].compute.table_from_matrix.columns must be non-empty"
            ));
        }
        _ => {}
    }
    Ok(())
}

fn validate_return_refs(
    ret: &PlanReturn,
    by_id: &HashMap<String, usize>,
) -> Result<ValidatedPlanReturn, String> {
    for id in return_refs(ret) {
        if !by_id.contains_key(id) {
            return Err(format!("plan.return references unknown id {id:?}"));
        }
    }
    match ret {
        PlanReturn::Node(id) => Ok(ValidatedPlanReturn::Node(NodeRef::new(id.clone())?)),
        PlanReturn::Parallel { parallel } => Ok(ValidatedPlanReturn::Parallel {
            parallel: parallel
                .iter()
                .cloned()
                .map(NodeRef::new)
                .collect::<Result<Vec<_>, _>>()?,
        }),
        PlanReturn::Record(map) => Ok(ValidatedPlanReturn::Record(
            map.iter()
                .map(|(k, v)| Ok((OutputName::new(k.clone())?, NodeRef::new(v.clone())?)))
                .collect::<Result<BTreeMap<_, _>, String>>()?,
        )),
    }
}

fn topological_order(plan: &Plan, adj: &[Vec<usize>]) -> Result<Vec<PlanNodeId>, String> {
    fn visit(
        i: usize,
        plan: &Plan,
        adj: &[Vec<usize>],
        mark: &mut [u8],
        out: &mut Vec<PlanNodeId>,
    ) -> Result<(), String> {
        if mark[i] == 2 {
            return Ok(());
        }
        if mark[i] == 1 {
            return Err("plan: depends_on has a cycle".to_string());
        }
        mark[i] = 1;
        for &d in &adj[i] {
            visit(d, plan, adj, mark, out)?;
        }
        mark[i] = 2;
        out.push(PlanNodeId::new(plan.nodes[i].id.clone())?);
        Ok(())
    }
    let mut mark = vec![0u8; plan.nodes.len()];
    let mut out = Vec::with_capacity(plan.nodes.len());
    for i in 0..plan.nodes.len() {
        visit(i, plan, adj, &mut mark, &mut out)?;
    }
    out.dedup();
    Ok(out)
}

fn validate_predicate(
    p: &PlanPredicate,
    node_index: usize,
    pred_index: usize,
) -> Result<(), String> {
    if p.field_path.is_empty() || p.field_path.iter().any(|s| s.trim().is_empty()) {
        return Err(format!(
            "plan.nodes[{node_index}].predicates[{pred_index}].field_path must be non-empty"
        ));
    }
    Ok(())
}

fn validate_plan_value_expr(
    value: &PlanValue,
    node_index: usize,
    path: &str,
) -> Result<(), String> {
    match value {
        PlanValue::Literal { .. }
        | PlanValue::Helper { .. }
        | PlanValue::Symbol { .. }
        | PlanValue::BindingSymbol { .. }
        | PlanValue::NodeSymbol { .. } => Ok(()),
        PlanValue::Template { input_bindings, .. } => {
            for b in input_bindings {
                if b.from.trim().is_empty() {
                    return Err(format!(
                        "plan.nodes[{node_index}].{path} has an empty binding"
                    ));
                }
            }
            Ok(())
        }
        PlanValue::Array { items } => {
            for (i, item) in items.iter().enumerate() {
                validate_plan_value_expr(item, node_index, &format!("{path}.items[{i}]"))?;
            }
            Ok(())
        }
        PlanValue::Object { fields } => {
            for (k, field) in fields {
                if k.trim().is_empty() {
                    return Err(format!(
                        "plan.nodes[{node_index}].{path} contains an empty object key"
                    ));
                }
                validate_plan_value_expr(field, node_index, &format!("{path}.{k}"))?;
            }
            Ok(())
        }
    }
}

/// Parse and validate Plan JSON.
pub fn validate_plan_value(plan: &serde_json::Value) -> Result<(), String> {
    let plan = parse_plan_value(plan)?;
    validate_plan(&plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_query_ok() {
        let v = serde_json::json!({
            "version": 1,
            "kind": "program",
            "name": "read-products",
            "nodes": [{
                "id": "n1",
                "kind": "query",
                "qualified_entity": { "entry_id": "acme", "entity": "Product" },
                "expr": "Product",
                "effect_class": "read",
                "result_shape": "list",
                "projection": [],
                "predicates": [{
                    "field_path": ["state"],
                    "op": "eq",
                    "value": { "kind": "literal", "value": "open" }
                }],
                "depends_on": [],
                "uses_result": []
            }],
            "return": "n1"
        });
        validate_plan_value(&v).expect("ok");
    }

    #[test]
    fn legacy_expr_list_normalizes_but_is_not_canonical() {
        let v = serde_json::json!({
            "nodes": [{ "expr": "x" }],
        });
        let p = parse_plan_value(&v).expect("legacy");
        assert_eq!(p.kind, PlanKind::Program);
        assert_eq!(p.nodes[0].id, "n1");
        validate_plan(&p).expect("legacy validates");
    }

    #[test]
    fn reject_cycle() {
        let v = serde_json::json!({
            "version": 1,
            "kind": "program",
            "name": "cycle",
            "nodes": [
                {
                    "id": "a",
                    "kind": "derive",
                    "effect_class": "artifact_read",
                    "result_shape": "artifact",
                    "depends_on": ["b"]
                },
                {
                    "id": "b",
                    "kind": "derive",
                    "effect_class": "artifact_read",
                    "result_shape": "artifact",
                    "depends_on": ["a"]
                }
            ],
            "return": "a"
        });
        assert!(validate_plan_value(&v).is_err());
    }

    #[test]
    fn reject_missing_qualified_entity() {
        let v = serde_json::json!({
            "version": 1,
            "kind": "program",
            "nodes": [{
                "id": "n1",
                "kind": "query",
                "expr": "Product",
                "effect_class": "read",
                "result_shape": "list"
            }],
            "return": "n1"
        });
        assert!(validate_plan_value(&v).is_err());
    }

    #[test]
    fn for_each_write_template_does_not_trust_agent_authored_approval() {
        let v = serde_json::json!({
            "version": 1,
            "kind": "program",
            "nodes": [
                {
                    "id": "find",
                    "kind": "query",
                    "qualified_entity": { "entry_id": "github", "entity": "Issue" },
                    "expr": "Issue{state=open}",
                    "effect_class": "read",
                    "result_shape": "list"
                },
                {
                    "id": "label",
                    "kind": "for_each",
                    "effect_class": "side_effect",
                    "result_shape": "side_effect_ack",
                    "source": "find",
                    "item_binding": "issue",
                    "depends_on": ["find"],
                    "uses_result": [{ "node": "find", "as": "issue" }],
                    "effect_template": {
                        "kind": "action",
                        "qualified_entity": { "entry_id": "github", "entity": "Issue" },
                        "expr_template": "Issue(${issue.id}).add-label(label=\"stale\")",
                        "effect_class": "side_effect",
                        "result_shape": "side_effect_ack"
                    }
                }
            ],
            "return": { "sourceIssues": "find", "labeledIssues": "label" }
        });
        validate_plan_value(&v).expect("host infers approval gates during dry-run");
    }

    #[test]
    fn compute_rejects_unknown_source_and_bad_aggregate() {
        let unknown_source = serde_json::json!({
            "version": 1,
            "kind": "program",
            "nodes": [{
                "id": "by_state",
                "kind": "compute",
                "effect_class": "artifact_read",
                "result_shape": "list",
                "compute": {
                    "source": "missing",
                    "op": { "kind": "group_by", "key": ["state"], "aggregates": [{ "name": "count", "function": "count" }] },
                    "schema": { "fields": [{ "name": "key", "value_kind": "string" }, { "name": "count", "value_kind": "integer" }] }
                }
            }],
            "return": "by_state"
        });
        assert!(validate_plan_value(&unknown_source).is_err());

        let bad_aggregate = serde_json::json!({
            "version": 1,
            "kind": "program",
            "nodes": [
                {
                    "id": "rows",
                    "kind": "data",
                    "effect_class": "artifact_read",
                    "result_shape": "artifact",
                    "data": { "kind": "literal", "value": [{ "points": 1 }] }
                },
                {
                    "id": "totals",
                    "kind": "compute",
                    "effect_class": "artifact_read",
                    "result_shape": "list",
                    "compute": {
                        "source": "rows",
                        "op": { "kind": "aggregate", "aggregates": [{ "name": "total", "function": "sum" }] },
                        "schema": { "fields": [{ "name": "total", "value_kind": "number" }] }
                    }
                }
            ],
            "return": "totals"
        });
        assert!(validate_plan_value(&bad_aggregate).is_err());
    }

    #[test]
    fn validated_plan_exposes_topology_and_typed_return_refs() {
        let v = serde_json::json!({
            "version": 1,
            "kind": "program",
            "nodes": [
                {
                    "id": "rows",
                    "kind": "data",
                    "effect_class": "artifact_read",
                    "result_shape": "artifact",
                    "data": { "kind": "literal", "value": [{ "state": "open" }] }
                },
                {
                    "id": "limited",
                    "kind": "compute",
                    "effect_class": "artifact_read",
                    "result_shape": "list",
                    "compute": {
                        "source": "rows",
                        "op": { "kind": "limit", "count": 1 },
                        "schema": { "fields": [{ "name": "state", "value_kind": "string", "source": ["state"] }] },
                        "page_size": 1
                    }
                }
            ],
            "return": { "limited": "limited" }
        });
        let plan = parse_plan_value(&v).expect("parse");
        let validated = validate_plan_artifact(&plan).expect("validate");
        assert_eq!(validated.topo[0].as_str(), "rows");
        assert_eq!(validated.topo[1].as_str(), "limited");
        assert_eq!(validated.return_value.refs()[0].as_str(), "limited");
    }
}
