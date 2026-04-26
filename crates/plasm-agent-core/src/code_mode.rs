//! **Plasm Code Mode** — TypeScript → JavaScript via the Oxc [transformer](https://oxc.rs/docs/guide/usage/transformer),
//! and (in hosts that link `rquickjs`) execution of that JS to build JSON plans for [`crate::mcp_plasm_code::run_code_mode_plan`].
//!
//! Enable the **`code_mode` Cargo feature** on `plasm-agent-core` (or `plasm-agent`) to compile this module and the
//! `oxc` / `rquickjs` optional dependencies.
//!
//! **Roles:** Oxc strips types and lowers syntax to ECMAScript a JS engine can run; [QuickJS](https://bellard.org/quickjs/)
//! (via [rquickjs](https://github.com/DelSkayn/rquickjs)) does **not** parse TypeScript — it runs the output of
//! [`transpile_typescript_to_javascript`] (or `tsc` / Node `oxc-transform`) only.

use std::path::Path;

use oxc::allocator::Allocator;
use oxc::ast::ast::{Argument, BindingPattern, CallExpression, Expression, Program, Statement};
use oxc::codegen::{Codegen, CodegenOptions};
use oxc::diagnostics::OxcDiagnostic;
use oxc::parser::Parser;
use oxc::semantic::SemanticBuilder;
use oxc::span::{GetSpan, SourceType};
use oxc::transformer::{HelperLoaderMode, TransformOptions, Transformer};
use serde::Serialize;
use std::collections::BTreeSet;

fn format_oxc_diagnostics(errors: Vec<OxcDiagnostic>, source: &str) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    for d in errors {
        let m = d.with_source_code(source.to_string());
        let _ = writeln!(&mut s, "{m}");
    }
    if let Some(guidance) = code_mode_syntax_guidance(source) {
        let _ = writeln!(&mut s, "\nCode Mode syntax guidance: {guidance}");
    }
    s
}

fn code_mode_syntax_guidance(source: &str) -> Option<&'static str> {
    if has_dangling_template_member_access(source) {
        return Some(
            "a tagged template is followed by a dangling `.`. Use a real field access such as `template`${node}`.id` / `template`${node}`.full_name`, or remove the dot. For Plan node fields, prefer direct AST-visible access like `node.id` inside a template.",
        );
    }
    None
}

fn has_dangling_template_member_access(source: &str) -> bool {
    let bytes = source.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'`' && bytes[i + 1] == b'.' {
            let mut j = i + 2;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j >= bytes.len() || matches!(bytes[j], b'}' | b')' | b',' | b';' | b']') {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Transpile a TypeScript (or TSX) string to ECMAScript using Oxc’s semantic + transformer
/// ([Oxc guide](https://oxc.rs/docs/guide/usage/transformer)).
///
/// `path_hint` is only used for language inference and stable codegen (e.g. `plan.ts` or `x.tsx`);
/// the source text is the second argument.
pub fn transpile_typescript_to_javascript(path_hint: &str, source: &str) -> Result<String, String> {
    let path = Path::new(path_hint);
    let source_type = SourceType::from_path(path).map_err(|e| format!("{e:?}"))?;
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, source, source_type).parse();
    if !ret.errors.is_empty() {
        return Err(format_oxc_diagnostics(ret.errors, source));
    }
    let mut program: Program = ret.program;

    let ret = SemanticBuilder::new()
        .with_excess_capacity(2.0)
        .with_enum_eval(true)
        .build(&program);
    if !ret.errors.is_empty() {
        return Err(format_oxc_diagnostics(ret.errors, source));
    }
    let scoping = ret.semantic.into_scoping();

    let mut transform_options = TransformOptions::enable_all();
    // Self-contained output for host embedders (e.g. QuickJS): no `import` and no required globals.
    transform_options.helper_loader.mode = HelperLoaderMode::Inline;

    let tret = Transformer::new(&allocator, path, &transform_options)
        .build_with_scoping(scoping, &mut program);
    if !tret.errors.is_empty() {
        return Err(format_oxc_diagnostics(tret.errors, source));
    }

    let options = CodegenOptions {
        ..CodegenOptions::default()
    };
    let printed = Codegen::new().with_options(options).build(&program);
    Ok(printed.code)
}

#[derive(Debug)]
struct SourceEdit {
    pos: usize,
    text: String,
}

#[derive(Debug, Default, Serialize)]
struct CodeModeAstPlanHints {
    node_ids: Vec<String>,
}

/// Inject Code Mode compiler hints derived from the TypeScript AST, not agent-authored aliases.
///
/// This keeps the public Plan DSL terse (`const issues = ...`) while still serializing stable
/// Plan node ids (`issues`) and symbolic item bindings (`issue.title`) into the synthesized DAG.
pub fn inject_plan_symbol_hints_typescript(
    path_hint: &str,
    source: &str,
) -> Result<String, String> {
    let path = Path::new(path_hint);
    let source_type = SourceType::from_path(path).map_err(|e| format!("{e:?}"))?;
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, source, source_type).parse();
    if !ret.errors.is_empty() {
        return Err(format_oxc_diagnostics(ret.errors, source));
    }

    let hints = derive_ast_plan_hints(&ret.program.body);
    let mut edits = Vec::new();
    collect_statement_edits(&ret.program.body, &mut edits);
    if !hints.node_ids.is_empty() {
        let encoded = serde_json::to_string(&hints).map_err(|e| e.to_string())?;
        edits.push(SourceEdit {
            pos: 0,
            text: format!("__plasmSetAstHints({encoded});\n"),
        });
    }
    if edits.is_empty() {
        return Ok(source.to_string());
    }
    edits.sort_by(|a, b| {
        b.pos
            .cmp(&a.pos)
            .then_with(|| b.text.len().cmp(&a.text.len()))
    });
    let mut out = source.to_string();
    for edit in edits {
        if edit.pos <= out.len() {
            out.insert_str(edit.pos, edit.text.as_str());
        }
    }
    Ok(out)
}

fn derive_ast_plan_hints(body: &[Statement<'_>]) -> CodeModeAstPlanHints {
    let mut node_ids = BTreeSet::new();
    for stmt in body {
        if let Statement::VariableDeclaration(decl) = stmt {
            for d in &decl.declarations {
                if let (Some(name), Some(init)) = (binding_identifier_name(&d.id), d.init.as_ref())
                {
                    if is_plan_node_initializer_with_known(init, &node_ids) {
                        node_ids.insert(name);
                    }
                }
            }
        }
    }
    CodeModeAstPlanHints {
        node_ids: node_ids.into_iter().collect(),
    }
}

fn is_plan_node_initializer_with_known(
    expr: &Expression<'_>,
    known_node_ids: &BTreeSet<String>,
) -> bool {
    match expr {
        Expression::CallExpression(call) => {
            is_plan_call_expression(call) || is_known_node_member_call(call, known_node_ids)
        }
        Expression::ParenthesizedExpression(p) => {
            is_plan_node_initializer_with_known(&p.expression, known_node_ids)
        }
        Expression::TSAsExpression(e) => {
            is_plan_node_initializer_with_known(&e.expression, known_node_ids)
        }
        Expression::TSSatisfiesExpression(e) => {
            is_plan_node_initializer_with_known(&e.expression, known_node_ids)
        }
        Expression::TSNonNullExpression(e) => {
            is_plan_node_initializer_with_known(&e.expression, known_node_ids)
        }
        Expression::TSInstantiationExpression(e) => {
            is_plan_node_initializer_with_known(&e.expression, known_node_ids)
        }
        _ => false,
    }
}

fn is_plan_node_initializer(expr: &Expression<'_>) -> bool {
    match expr {
        Expression::CallExpression(call) => is_plan_call_expression(call),
        Expression::ParenthesizedExpression(p) => is_plan_node_initializer(&p.expression),
        Expression::TSAsExpression(e) => is_plan_node_initializer(&e.expression),
        Expression::TSSatisfiesExpression(e) => is_plan_node_initializer(&e.expression),
        Expression::TSNonNullExpression(e) => is_plan_node_initializer(&e.expression),
        Expression::TSInstantiationExpression(e) => is_plan_node_initializer(&e.expression),
        _ => false,
    }
}

fn is_known_node_member_call(call: &CallExpression<'_>, known_node_ids: &BTreeSet<String>) -> bool {
    match &call.callee {
        Expression::StaticMemberExpression(member) => {
            matches!(&member.object, Expression::Identifier(id) if known_node_ids.contains(id.name.as_str()))
        }
        Expression::ChainExpression(chain) => match &chain.expression {
            oxc::ast::ast::ChainElement::StaticMemberExpression(member) => {
                matches!(&member.object, Expression::Identifier(id) if known_node_ids.contains(id.name.as_str()))
            }
            _ => false,
        },
        _ => false,
    }
}

fn argument_contains_plan_node_initializer(arg: &Argument<'_>) -> bool {
    match arg {
        Argument::CallExpression(call) => is_plan_call_expression(call),
        Argument::ParenthesizedExpression(p) => is_plan_node_initializer(&p.expression),
        Argument::TSAsExpression(e) => is_plan_node_initializer(&e.expression),
        Argument::TSSatisfiesExpression(e) => is_plan_node_initializer(&e.expression),
        Argument::TSNonNullExpression(e) => is_plan_node_initializer(&e.expression),
        Argument::TSInstantiationExpression(e) => is_plan_node_initializer(&e.expression),
        _ => false,
    }
}

fn is_plan_call_expression(call: &CallExpression<'_>) -> bool {
    is_static_callee(&call.callee, "Plan", "map")
        || is_static_callee(&call.callee, "Plan", "project")
        || is_static_callee(&call.callee, "Plan", "filter")
        || is_static_callee(&call.callee, "Plan", "aggregate")
        || is_static_callee(&call.callee, "Plan", "groupBy")
        || is_static_callee(&call.callee, "Plan", "sort")
        || is_static_callee(&call.callee, "Plan", "limit")
        || is_static_callee(&call.callee, "Plan", "table")
        || is_static_callee(&call.callee, "Plan", "data")
        || is_static_callee(&call.callee, "Plan", "read")
        || is_member_callee_named(call, "get")
        || is_member_callee_named(call, "query")
        || is_member_callee_named(call, "search")
        || is_member_callee_named(call, "count")
        || call
            .arguments
            .iter()
            .any(argument_contains_plan_node_initializer)
}

fn is_member_callee_named(call: &CallExpression<'_>, property_name: &str) -> bool {
    match &call.callee {
        Expression::StaticMemberExpression(member) => {
            member.property.name.as_str() == property_name
        }
        Expression::ChainExpression(chain) => match &chain.expression {
            oxc::ast::ast::ChainElement::StaticMemberExpression(member) => {
                member.property.name.as_str() == property_name
            }
            _ => false,
        },
        _ => false,
    }
}

fn collect_statement_edits<'a>(body: &[Statement<'a>], edits: &mut Vec<SourceEdit>) {
    for stmt in body {
        match stmt {
            Statement::VariableDeclaration(decl) => {
                for d in &decl.declarations {
                    if let (Some(name), Some(init)) =
                        (binding_identifier_name(&d.id), d.init.as_ref())
                    {
                        let span = init.span();
                        edits.push(SourceEdit {
                            pos: span.start as usize,
                            text: "__plasmBind((".to_string(),
                        });
                        edits.push(SourceEdit {
                            pos: span.end as usize,
                            text: format!("), {name:?})"),
                        });
                        collect_expression_edits(init, edits);
                    }
                }
            }
            Statement::ExpressionStatement(expr) => {
                collect_expression_edits(&expr.expression, edits)
            }
            _ => {}
        }
    }
}

fn collect_expression_edits<'a>(expr: &Expression<'a>, edits: &mut Vec<SourceEdit>) {
    match expr {
        Expression::CallExpression(call) => {
            collect_call_binding_edits(call, edits);
            collect_expression_edits(&call.callee, edits);
            for arg in &call.arguments {
                collect_argument_edits(arg, edits);
            }
        }
        Expression::StaticMemberExpression(member) => {
            collect_expression_edits(&member.object, edits)
        }
        Expression::ComputedMemberExpression(member) => {
            collect_expression_edits(&member.object, edits);
            collect_expression_edits(&member.expression, edits);
        }
        Expression::ParenthesizedExpression(p) => collect_expression_edits(&p.expression, edits),
        Expression::TSAsExpression(e) => collect_expression_edits(&e.expression, edits),
        Expression::TSSatisfiesExpression(e) => collect_expression_edits(&e.expression, edits),
        Expression::TSNonNullExpression(e) => collect_expression_edits(&e.expression, edits),
        Expression::TSInstantiationExpression(e) => collect_expression_edits(&e.expression, edits),
        _ => {}
    }
}

fn collect_argument_edits<'a>(arg: &Argument<'a>, edits: &mut Vec<SourceEdit>) {
    match arg {
        Argument::CallExpression(call) => {
            collect_call_binding_edits(call, edits);
            collect_expression_edits(&call.callee, edits);
            for arg in &call.arguments {
                collect_argument_edits(arg, edits);
            }
        }
        Argument::ArrowFunctionExpression(_) => {}
        Argument::StaticMemberExpression(member) => collect_expression_edits(&member.object, edits),
        Argument::ComputedMemberExpression(member) => {
            collect_expression_edits(&member.object, edits);
            collect_expression_edits(&member.expression, edits);
        }
        _ => {}
    }
}

fn collect_call_binding_edits<'a>(call: &CallExpression<'a>, edits: &mut Vec<SourceEdit>) {
    if !(is_identifier_callee(&call.callee, "forEach")
        || is_static_callee(&call.callee, "Plan", "map"))
    {
        return;
    }
    if call.arguments.len() != 2 {
        return;
    }
    let Some(binding) = arrow_first_param_name(&call.arguments[1]) else {
        return;
    };
    let span = call.arguments[1].span();
    edits.push(SourceEdit {
        pos: span.start as usize,
        text: format!("{binding:?}, "),
    });
}

fn is_identifier_callee(expr: &Expression<'_>, expected: &str) -> bool {
    matches!(expr, Expression::Identifier(id) if id.name.as_str() == expected)
}

fn is_static_callee(expr: &Expression<'_>, object_name: &str, property_name: &str) -> bool {
    match expr {
        Expression::StaticMemberExpression(member) => {
            matches!(&member.object, Expression::Identifier(id) if id.name.as_str() == object_name)
                && member.property.name.as_str() == property_name
        }
        Expression::ChainExpression(chain) => match &chain.expression {
            oxc::ast::ast::ChainElement::StaticMemberExpression(member) => {
                matches!(&member.object, Expression::Identifier(id) if id.name.as_str() == object_name)
                    && member.property.name.as_str() == property_name
            }
            _ => false,
        },
        _ => false,
    }
}

fn arrow_first_param_name(arg: &Argument<'_>) -> Option<String> {
    let Argument::ArrowFunctionExpression(arrow) = arg else {
        return None;
    };
    let first = arrow.params.items.first()?;
    binding_identifier_name(&first.pattern)
}

fn binding_identifier_name(binding: &BindingPattern<'_>) -> Option<String> {
    match binding {
        BindingPattern::BindingIdentifier(id) => Some(id.name.as_str().to_string()),
        BindingPattern::AssignmentPattern(assign) => binding_identifier_name(&assign.left),
        _ => None,
    }
}

// ---- QuickJS sandbox (optional `code_mode` dependency) -----------------------------------------

/// Owns a QuickJS engine for **guest** user code. No filesystem, network, or host APIs — only
/// Oxc-transpiled JS plus an optional `bootstrap` snippet (e.g. from
/// `plasm_facade_gen::quickjs_runtime_module_bootstrap`).
pub struct CodeModeSandbox {
    runtime: rquickjs::Runtime,
}

impl CodeModeSandbox {
    /// New isolate (empty global object except QuickJS built-ins).
    pub fn new() -> Result<Self, String> {
        rquickjs::Runtime::new()
            .map_err(|e| e.to_string())
            .map(|runtime| Self { runtime })
    }

    /// Run already-transpiled JavaScript. The final expression (or last statement) must yield a
    /// **string** containing JSON; that string is parsed to [`serde_json::Value`].
    ///
    /// `bootstrap` is prepended as untyped ESM-flattened JS (`export function` → `function`) if set.
    pub fn eval_javascript_to_json_value(
        &self,
        transpiled_js: &str,
        bootstrap: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        let ctx = rquickjs::Context::full(&self.runtime).map_err(|e| e.to_string())?;
        ctx.with(|ctx| {
            if let Some(b) = bootstrap {
                let flat = flatten_esm_bootstrap(b);
                let _: () = ctx.eval(flat.as_str()).map_err(|e| e.to_string())?;
            }
            let encoded_js = serde_json::to_string(transpiled_js).map_err(|e| e.to_string())?;
            let wrapped = format!(
                "try {{ eval({encoded_js}) }} catch (e) {{ JSON.stringify({{ __plasmCodeModeError: String((e && e.message) || e) }}) }}"
            );
            let out: String = ctx.eval(wrapped.as_str()).map_err(|e| e.to_string())?;
            let value: serde_json::Value =
                serde_json::from_str(out.trim()).map_err(|e| format!("plan JSON: {e}"))?;
            if let Some(message) = value
                .get("__plasmCodeModeError")
                .and_then(serde_json::Value::as_str)
            {
                return Err(format!("Code Mode DSL error: {message}"));
            }
            Ok(value)
        })
    }

    /// [`transpile_typescript_to_javascript`] then [`Self::eval_javascript_to_json_value`].
    pub fn eval_typescript_to_json_value(
        &self,
        path_hint: &str,
        typescript: &str,
        bootstrap: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        let hinted = inject_plan_symbol_hints_typescript(path_hint, typescript)?;
        let js = transpile_typescript_to_javascript(path_hint, hinted.as_str())?;
        self.eval_javascript_to_json_value(&js, bootstrap)
    }
}

/// Strip ESM `export` so a bootstrap module can be `eval`’d in an expression context.
fn flatten_esm_bootstrap(js: &str) -> String {
    js.replace("export function", "function")
        .replace("export class", "class")
        .replace("export const", "const")
        .replace("export let", "let")
        .replace("export var", "var")
        .replace("export {", "{")
}

#[cfg(test)]
mod tests {
    use super::{
        inject_plan_symbol_hints_typescript, transpile_typescript_to_javascript, CodeModeSandbox,
    };

    #[test]
    fn transpile_strips_type_only_annotations() {
        let out = transpile_typescript_to_javascript(
            "k.ts",
            "const plan: { version: number; nodes: { expr: string }[] } = { version: 1, nodes: [{ expr: 'Product' }] };",
        )
        .expect("transpile");
        assert!(
            out.contains("version") && out.contains("Product") && !out.contains(": { version"),
            "expected stripped type annotations, got: {out}"
        );
    }

    #[test]
    fn parse_errors_explain_dangling_template_member_access() {
        let err = inject_plan_symbol_hints_typescript(
            "plan.ts",
            "const key = template`${oneRepo}`. } as any;",
        )
        .expect_err("dangling template member access should not parse");

        assert!(err.contains("Code Mode syntax guidance"), "{err}");
        assert!(err.contains("dangling `.`"), "{err}");
        assert!(err.contains("template`${node}`.id"), "{err}");
    }

    #[test]
    fn sandbox_json_roundtrip() {
        let s = CodeModeSandbox::new().expect("runtime");
        let v = s
            .eval_javascript_to_json_value(
                "JSON.stringify({ version: 1, nodes: [{ expr: 'A' }] })",
                None,
            )
            .expect("eval");
        assert_eq!(v["version"], 1);
        assert_eq!(v["nodes"][0]["expr"], "A");
    }

    #[test]
    fn sandbox_reports_quickjs_dsl_errors() {
        let s = CodeModeSandbox::new().expect("runtime");
        let err = s
            .eval_typescript_to_json_value(
                "plan.ts",
                "const Product = makeEntity('acme', 'Product'); Plan.return(Product.get('p1').select());",
                Some(&plasm_facade_gen::quickjs_runtime_module_bootstrap()),
            )
            .expect_err("empty select rejected");
        assert!(err.contains("Code Mode DSL error"), "{err}");
        assert!(
            err.contains("select(...) requires at least one field"),
            "{err}"
        );
    }

    #[test]
    fn ast_hints_authorize_cross_node_plan_field_refs() {
        let source = r#"
const base = Plan.data([{ name: "pikachu" }]);
const moveFacts = Plan.data([{ move: "thunderbolt", power: 90 }]);
const cards = Plan.map(base, (p) => ({
  body: template`${p.name} uses ${moveFacts.move}`,
  power: moveFacts.power,
}));
Plan.return({ cards });
"#;
        let hinted = inject_plan_symbol_hints_typescript("plan.ts", source).expect("hints");
        assert!(hinted.contains("__plasmSetAstHints"));
        assert!(hinted.contains("\"moveFacts\""));

        let s = CodeModeSandbox::new().expect("runtime");
        let plan = s
            .eval_typescript_to_json_value(
                "plan.ts",
                source,
                Some(&plasm_facade_gen::quickjs_runtime_module_bootstrap()),
            )
            .expect("eval");
        let cards = plan["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "cards")
            .expect("cards");
        let inputs = cards["derive_template"]["inputs"].as_array().unwrap();
        assert_eq!(inputs[0]["node"], "moveFacts");
        assert_eq!(
            cards["derive_template"]["value"]["fields"]["power"]["kind"],
            "node_symbol"
        );
        crate::code_mode_plan::validate_plan_value(&plan).expect("serialized plan validates");
    }
}
