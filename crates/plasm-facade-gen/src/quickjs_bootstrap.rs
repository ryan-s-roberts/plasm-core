//! Small JavaScript module (ESM shape) for QuickJS — generated with **genco** to keep the dependency manifest.

use std::fmt::Write as _;

use crate::delta::FacadeDeltaV1;

/// QuickJS helpers for building one **Plan** artifact (no host I/O).
pub fn quickjs_runtime_module_bootstrap() -> String {
    r#"
        export function entityRef(api, entity, key) {
            return { api, entity, key };
        }
        export function toPlasmExpr(surface) {
            return String(surface);
        }

        let __plasmAnonSeq = 0;
        let __plasmAstHints = { node_ids: [] };

        export function __plasmSetAstHints(hints) {
            __plasmAstHints.node_ids = Array.isArray(hints && hints.node_ids) ? hints.node_ids.map(String) : [];
        }

        function __anonId() {
            __plasmAnonSeq += 1;
            return "__anon_" + __plasmAnonSeq;
        }

        function __isSpecial(v) {
            return v && typeof v === "object" && (v.__plasmExpr || v.__planValue || v.__bindingPath || v.__planNodeId || v.__toPlanHandle);
        }

        function __valueMeta(v) {
            if (v && v.__planValue) return v.__planValue;
            if (v && v.__bindingPath) return { kind: "binding_symbol", binding: v.__bindingName || String(v.__bindingPath).split(".")[0], path: v.__bindingFieldPath || [] };
            if (v && v.__planNodeId) return { kind: "symbol", path: v.__planNodeId };
            if (Array.isArray(v)) return { kind: "array", items: v.map(__valueMeta) };
            if (v && typeof v === "object" && !__isSpecial(v)) {
                const fields = {};
                for (const k of Object.keys(v)) fields[k] = __valueMeta(v[k]);
                return { kind: "object", fields };
            }
            return { kind: "literal", value: v };
        }

        function __quote(v) {
            if (v && v.__plasmExpr) return v.__plasmExpr;
            if (v && v.__bindingPath) return "${" + v.__bindingPath + "}";
            if (typeof v === "number" || typeof v === "boolean") return String(v);
            if (Array.isArray(v)) return "[" + v.map(__quote).join(",") + "]";
            if (v && typeof v === "object") {
                return "{" + Object.keys(v).map(k => k + "=" + __quote(v[k])).join(", ") + "}";
            }
            return JSON.stringify(String(v));
        }

        function __displayPlanValue(v) {
            if (!v) return "";
            if (v.kind === "binding_symbol") return [v.binding].concat(v.path || []).filter(Boolean).join(".");
            if (v.kind === "node_symbol") return [v.alias || v.node].concat(v.path || []).filter(Boolean).join(".");
            if (v.kind === "symbol") return v.path || "";
            return "";
        }

        function __fieldPath(field) {
            return String(field).split(".").filter(Boolean);
        }

        function __unsupportedProjectionMethod(name) {
            return function() {
                throw new Error("Plan.project callbacks support field-path expressions only; unsupported array/string method `" + name + "`. Project a path such as item.types[0].type.name, then use Plan.map or a later supported scalar transform for richer computation.");
            };
        }

        function __pathFromSymbol(value, binding) {
            if (!value || !value.__bindingPath) {
                throw new Error("Plan.project callback must return a symbolic field path (for example item.types[0].type.name). Unsupported: literals, object construction, loops, .map/.filter/.join, or arbitrary function calls.");
            }
            const raw = String(value.__bindingPath);
            const prefix = binding + ".";
            return raw === binding ? [] : (raw.startsWith(prefix) ? raw.slice(prefix.length) : raw).split(".").filter(Boolean);
        }

        function __schemaFromFields(entity, fields, sourcePaths) {
            return {
                entity,
                fields: fields.map((name, i) => ({
                    name,
                    value_kind: "unknown",
                    source: sourcePaths && sourcePaths[i] ? sourcePaths[i] : undefined,
                })),
            };
        }

        function __predicateExpr(p) {
            const lhs = p.field_path.join(".");
            if (p.op === "eq") return lhs + "=" + __quoteFromPlanValue(p.value);
            if (p.op === "ne") return lhs + "!=" + __quoteFromPlanValue(p.value);
            if (p.op === "lt") return lhs + "<" + __quoteFromPlanValue(p.value);
            if (p.op === "lte") return lhs + "<=" + __quoteFromPlanValue(p.value);
            if (p.op === "gt") return lhs + ">" + __quoteFromPlanValue(p.value);
            if (p.op === "gte") return lhs + ">=" + __quoteFromPlanValue(p.value);
            if (p.op === "contains") return lhs + "~" + __quoteFromPlanValue(p.value);
            return lhs + "=" + __quoteFromPlanValue(p.value);
        }

        function __quoteFromPlanValue(v) {
            if (!v || v.kind === "literal") return __quote(v ? v.value : null);
            if (v.kind === "helper") return v.display ? JSON.stringify(String(v.display)) : String(v.name) + "(" + (v.args || []).map(__quote).join(",") + ")";
            if (v.kind === "symbol") return "${" + v.path + "}";
            if (v.kind === "binding_symbol" || v.kind === "node_symbol") return "${" + __displayPlanValue(v) + "}";
            if (v.kind === "template") return "template(" + JSON.stringify(v.template) + ")";
            return JSON.stringify(String(v));
        }

        function __filterPredicates(obj) {
            return Object.keys(obj || {}).map(k => ({
                field_path: __fieldPath(k),
                op: "eq",
                value: __valueMeta(obj[k]),
            }));
        }

        function __filters(obj, extraPredicates) {
            const keys = Object.keys(obj || {});
            const parts = keys.map(k => k + "=" + __quote(obj[k]));
            for (const p of (extraPredicates || [])) parts.push(__predicateExpr(p));
            if (parts.length === 0) return "";
            return "{" + parts.join(", ") + "}";
        }

        function __normalizeReturn(v) {
            if (v && v.__planNodeId) return v.__planNodeId;
            if (v && Array.isArray(v.parallel)) return { parallel: v.parallel.map(String) };
            if (Array.isArray(v)) return { parallel: v.map(__normalizeReturn) };
            if (v && typeof v === "object") {
                const out = {};
                for (const k of Object.keys(v)) out[k] = __normalizeReturn(v[k]);
                return out;
            }
            return String(v);
        }

        function __collectNodes(v, out) {
            if (!v) return;
            if (v.__planNodes) {
                for (const n of v.__planNodes) out.push(n);
                return;
            }
            if (Array.isArray(v)) {
                for (const x of v) __collectNodes(x, out);
                return;
            }
            if (typeof v === "object") {
                for (const k of Object.keys(v)) __collectNodes(v[k], out);
            }
        }

        function __nodeHandle(id, node) {
            const nid = id || __anonId();
            const h = { __planNodeId: nid };
            if (node) h.__planNodes = [Object.assign({}, node, { id: nid })];
            return __nodeHandleProxy(h);
        }

        function __assignHandleId(h, id) {
            if (!h || !id || !h.__planNodes || h.__planNodes.length !== 1) return h;
            h.__planNodeId = String(id);
            h.__planNodes[0].id = String(id);
            return __nodeHandleProxy(h);
        }

        function __plasmBind(value, id) {
            if (value && value.__toPlanHandle) return value.__toPlanHandle(String(id));
            if (value && value.__planNodeId) return __assignHandleId(value, String(id));
            if (value && value.kind && value.effect_class) return __nodeHandle(String(id), value);
            return value;
        }

        function __symbol(path) {
            const parts = String(path).split(".").filter(Boolean);
            const binding = parts.shift() || String(path);
            return new Proxy({ __bindingPath: path, __bindingName: binding, __bindingFieldPath: parts, __plasmExpr: "${" + path + "}" }, {
                get(target, prop) {
                    if (prop in target) return target[prop];
                    if (typeof prop === "symbol") return target[prop];
                    if (prop === "__planValue" || prop === "__planNodeId" || prop === "__toPlanHandle" || prop === "__planNodes") return undefined;
                    if (["map", "filter", "join", "reduce", "flatMap", "forEach"].includes(String(prop))) return __unsupportedProjectionMethod(String(prop));
                    return __symbol(path + "." + String(prop));
                }
            });
        }

        function __nodeHandleProxy(handle, cardinality) {
            return new Proxy(handle, {
                get(target, prop) {
                    if (prop in target) return target[prop];
                    if (typeof prop === "symbol") return target[prop];
                    if (prop === "__planValue" || prop === "__toPlanHandle") return undefined;
                    const node = String(target.__planNodeId);
                    if (!__plasmAstHints.node_ids.includes(node)) {
                        throw new Error("Plan node field access is not AST-authorized for `" + node + "`");
                    }
                    return __nodeRef(target, node, String(prop), cardinality || "auto");
                }
            });
        }

        function __nodeRef(handle, node, path, cardinality) {
            const parts = String(path).split(".").filter(Boolean);
            const value = { kind: "node_symbol", node, alias: node, path: parts, cardinality: cardinality || "auto" };
            const ref = {
                __planValue: value,
                __planNodes: handle && handle.__planNodes ? handle.__planNodes : undefined,
                __plasmExpr: "${" + [node].concat(parts).join(".") + "}",
                __nodeInput: { node, alias: node, cardinality: cardinality || "auto" },
            };
            return new Proxy(ref, {
                get(target, prop) {
                    if (prop in target) return target[prop];
                    if (typeof prop === "symbol") return target[prop];
                    if (prop === "__bindingPath" || prop === "__bindingName" || prop === "__bindingFieldPath" || prop === "__planNodeId" || prop === "__toPlanHandle") return undefined;
                    return __nodeRef(handle, node, parts.concat(String(prop)).join("."), cardinality || "auto");
                }
            });
        }

        function __planSingleton(handle) {
            if (!handle || !handle.__planNodeId) throw new Error("Plan.singleton expects a Plan node");
            return __nodeHandleProxy(handle, "singleton");
        }

        function __nodeInputsFromPlanValue(value, out) {
            if (!value || typeof value !== "object") return;
            if (value.kind === "node_symbol") {
                const key = String(value.alias || value.node);
                if (!out[key]) out[key] = { node: value.node, alias: key, cardinality: value.cardinality || "auto" };
                return;
            }
            if (value.kind === "template") {
                for (const b of (value.input_bindings || [])) {
                    if (b.node) {
                        const key = String(b.alias || b.node);
                        if (!out[key]) out[key] = { node: b.node, alias: key, cardinality: b.cardinality || "auto" };
                    }
                }
                return;
            }
            if (value.kind === "array") {
                for (const item of (value.items || [])) __nodeInputsFromPlanValue(item, out);
                return;
            }
            if (value.kind === "object") {
                for (const field of Object.values(value.fields || {})) __nodeInputsFromPlanValue(field, out);
            }
        }

        function __nodeInputsFromValueMeta(value) {
            const out = {};
            __nodeInputsFromPlanValue(value, out);
            return Object.values(out);
        }

        export class Plan {
            constructor(name) {
                this.version = 1;
                this.kind = "program";
                this.name = name || null;
                this.nodes = [];
                this._return = null;
            }
            static named(name) {
                return new Plan(name);
            }
            static return(value) {
                return new Plan(null).return(value);
            }
            static read(idOrEffect, maybeEffect) {
                if (typeof idOrEffect === "string") return __nodeHandle(idOrEffect, maybeEffect);
                const effect = idOrEffect;
                return {
                    __toPlanHandle(id) {
                        return __nodeHandle(id, effect);
                    }
                };
            }
            static data(value) {
                return {
                    __toPlanHandle(id) {
                        return __nodeHandle(id, {
                            kind: "data",
                            effect_class: "artifact_read",
                            result_shape: "artifact",
                            data: __valueMeta(value),
                            depends_on: [],
                            uses_result: [],
                        });
                    }
                };
            }
            static map(source, fn) {
                return Plan._map.apply(null, arguments);
            }
            static singleton(source) {
                return __planSingleton(source);
            }
            static _map(source, fn) {
                const sourceId = __normalizeReturn(source);
                let binding = "item";
                if (typeof fn === "string") {
                    binding = fn;
                    fn = arguments[2];
                }
                const item = __symbol(binding);
                const value = fn(item);
                const valueMeta = __valueMeta(value);
                const inputs = __nodeInputsFromValueMeta(valueMeta);
                return {
                    __toPlanHandle(id) {
                        const nodes = [];
                        __collectNodes(source, nodes);
                        __collectNodes(value, nodes);
                        nodes.push({
                            id,
                            kind: "derive",
                            effect_class: "artifact_read",
                            result_shape: "artifact",
                            source: sourceId,
                            item_binding: binding,
                            derive_template: {
                                kind: "map",
                                source: sourceId,
                                item_binding: binding,
                                inputs,
                                value: valueMeta,
                            },
                            depends_on: [sourceId].concat(inputs.map(i => i.node)),
                            uses_result: [{ node: sourceId, as: binding }].concat(inputs.map(i => ({ node: i.node, as: i.alias }))),
                        });
                        const handle = __nodeHandle(id, nodes[nodes.length - 1]);
                        handle.__planNodes = nodes;
                        return handle;
                    }
                };
            }
            static project(source, spec) {
                const sourceId = __normalizeReturn(source);
                const binding = "item";
                const item = __symbol(binding);
                const fields = {};
                const sourcePaths = [];
                if (Array.isArray(spec)) {
                    for (const name of spec) {
                        const path = __fieldPath(name);
                        fields[String(name)] = path;
                        sourcePaths.push(path);
                    }
                } else {
                    for (const [name, fn] of Object.entries(spec || {})) {
                        if (typeof fn !== "function") throw new Error("Plan.project spec values must be callbacks");
                        const path = __pathFromSymbol(fn(item), binding);
                        fields[name] = path;
                        sourcePaths.push(path);
                    }
                }
                return Plan._compute(source, { kind: "project", fields }, __schemaFromFields("PlanProject", Object.keys(fields), sourcePaths));
            }
            static filter(source, ...predicates) {
                const sourceId = __normalizeReturn(source);
                return Plan._compute(source, { kind: "filter", predicates: predicates.flat() }, { entity: "PlanFilter", fields: [{ name: "value", value_kind: "unknown" }] });
            }
            static aggregate(source, aggregates) {
                const sourceId = __normalizeReturn(source);
                return Plan._compute(source, { kind: "aggregate", aggregates: aggregates || [] }, __schemaFromFields("PlanAggregate", (aggregates || []).map(a => a.name), []));
            }
            static groupBy(source, keyFn) {
                const sourceId = __normalizeReturn(source);
                const binding = "item";
                const item = __symbol(binding);
                const key = __pathFromSymbol(keyFn(item), binding);
                return {
                    count(name) {
                        const countName = name || "count";
                        return Plan._compute(source, { kind: "group_by", key, aggregates: [{ name: countName, function: "count" }] }, {
                            entity: "PlanGroup",
                            fields: [
                                { name: "key", value_kind: "unknown", source: key },
                                { name: countName, value_kind: "integer" },
                            ],
                        });
                    },
                    aggregate(aggregates) {
                        return Plan._compute(source, { kind: "group_by", key, aggregates: aggregates || [] }, __schemaFromFields("PlanGroup", ["key"].concat((aggregates || []).map(a => a.name)), [key]));
                    }
                };
            }
            static sort(source, keyFn, direction) {
                const sourceId = __normalizeReturn(source);
                const binding = "item";
                const key = __pathFromSymbol(keyFn(__symbol(binding)), binding);
                return Plan._compute(source, { kind: "sort", key, descending: direction === "desc" }, { entity: "PlanSort", fields: [{ name: "value", value_kind: "unknown" }] });
            }
            static limit(source, count) {
                const sourceId = __normalizeReturn(source);
                return Plan._compute(source, { kind: "limit", count: Number(count) }, { entity: "PlanLimit", fields: [{ name: "value", value_kind: "unknown" }] });
            }
            static table(source, spec) {
                const sourceId = __normalizeReturn(source);
                const columns = (spec && spec.columns) || [];
                return Plan._compute(source, { kind: "table_from_matrix", columns, has_header: !!(spec && spec.hasHeader) }, __schemaFromFields("PlanTable", columns, []));
            }
            static _compute(source, op, schema) {
                const sourceId = __normalizeReturn(source);
                return {
                    __toPlanHandle(id) {
                        const nodes = [];
                        __collectNodes(source, nodes);
                        nodes.push({
                            id,
                            kind: "compute",
                            effect_class: "artifact_read",
                            result_shape: "list",
                            compute: {
                                source: sourceId,
                                op,
                                schema,
                                page_size: 50,
                            },
                            depends_on: [sourceId],
                            uses_result: [{ node: sourceId, as: "source" }],
                        });
                        const handle = __nodeHandle(id, nodes[nodes.length - 1]);
                        handle.__planNodes = nodes;
                        return handle;
                    }
                };
            }
            stage(id, effect) {
                const node = Object.assign({}, effect, { id });
                this.nodes.push(node);
                return __nodeHandle(id, node);
            }
            parallel(id, ...effects) {
                const ids = effects.map((e, i) => {
                    const nid = e && e.__planNodeId ? e.__planNodeId : id + "_" + (i + 1);
                    if (!e.__planNodeId) this.stage(nid, e);
                    else __collectNodes(e, this.nodes);
                    return nid;
                });
                this._return = { parallel: ids };
                return __nodeHandle(id);
            }
            dependsOn(nodeId, dependencyId) {
                const node = this.nodes.find(n => n.id === String(nodeId));
                if (node) {
                    node.depends_on = node.depends_on || [];
                    node.depends_on.push(String(dependencyId));
                }
                return this;
            }
            return(value) {
                __collectNodes(value, this.nodes);
                const seen = new Set();
                this.nodes = this.nodes.filter(n => {
                    if (seen.has(n.id)) return false;
                    seen.add(n.id);
                    return true;
                });
                this._return = __normalizeReturn(value);
                const out = {
                    version: this.version,
                    kind: this.kind,
                    nodes: this.nodes,
                    return: this._return,
                };
                if (this.name) out.name = this.name;
                return JSON.stringify(out);
            }
        }

        export function field(name) {
            const path = __fieldPath(name);
            const make = (op, value) => ({ field_path: path, op, value: __valueMeta(value) });
            return {
                eq: v => make("eq", v),
                ne: v => make("ne", v),
                lt: v => make("lt", v),
                lte: v => make("lte", v),
                gt: v => make("gt", v),
                gte: v => make("gte", v),
                contains: v => make("contains", v),
                in: v => make("in", v),
            };
        }

        export function daysAgo(days) {
            return {
                __plasmExpr: JSON.stringify(String(days) + "d"),
                __planValue: { kind: "helper", name: "daysAgo", args: [days], display: String(days) + "d" },
            };
        }

        export function repo(owner, repo) {
            return { owner, repo };
        }

        export function linearTeam(id) {
            return String(id);
        }

        export function template(strings, ...values) {
            let raw = "";
            const input_bindings = [];
            for (let i = 0; i < strings.length; i++) {
                raw += strings[i];
                if (i < values.length) {
                    const v = values[i];
                    if (v && v.__bindingPath) {
                        raw += "${" + v.__bindingPath + "}";
                        input_bindings.push({ from: v.__bindingPath, to: "" });
                    } else if (v && v.__planValue && v.__planValue.kind === "node_symbol") {
                        const display = __displayPlanValue(v.__planValue);
                        raw += "${" + display + "}";
                        input_bindings.push({ from: display, to: "", node: v.__planValue.node, alias: v.__planValue.alias, cardinality: v.__nodeInput ? v.__nodeInput.cardinality : "auto" });
                    } else {
                        raw += String(v);
                    }
                }
            }
            return {
                __plasmExpr: "template(" + JSON.stringify(raw) + ")",
                __planValue: { kind: "template", template: raw, input_bindings },
                input_bindings,
            };
        }

        function __bindingsFromInput(input) {
            const out = [];
            for (const [k, v] of Object.entries(input || {})) {
                if (v && v.__bindingPath) out.push({ from: v.__bindingPath, to: k });
                if (v && v.__planValue && v.__planValue.kind === "template") {
                    for (const b of (v.__planValue.input_bindings || [])) out.push({ from: b.from, to: k });
                }
            }
            return out;
        }

        function __callArgs(input) {
            const keys = Object.keys(input || {});
            if (keys.length === 0) return "";
            return "(" + keys.map(k => k + "=" + __quote(input[k])).join(", ") + ")";
        }

        export function makeEntity(entry_id, entity) {
            return {
                query(filters) {
                    let projection = [];
                    let predicates = __filterPredicates(filters || {});
                    return {
                        where(...ps) {
                            predicates = predicates.concat(ps.flat());
                            return this;
                        },
                        select(...fields) {
                            projection = fields;
                            return this;
                        },
                        yield() {
                            const exprBase = entity + __filters(filters || {}, predicates.slice(Object.keys(filters || {}).length));
                            const expr = projection.length ? exprBase + "[" + projection.join(",") + "]" : exprBase;
                            return {
                                kind: "query",
                                qualified_entity: { entry_id, entity },
                                expr,
                                effect_class: "read",
                                result_shape: "list",
                                projection,
                                predicates,
                                depends_on: [],
                                uses_result: [],
                            };
                        },
                        as(id) {
                            return __nodeHandle(id, Object.assign(this.yield(), { id }));
                        },
                        __toPlanHandle(id) {
                            return __nodeHandle(id, this.yield());
                        },
                    };
                },
                get(id) {
                    return {
                        kind: "get",
                        qualified_entity: { entry_id, entity },
                        expr: entity + "(" + __quote(id) + ")",
                        effect_class: "read",
                        result_shape: "single",
                        projection: [],
                        predicates: [],
                        depends_on: [],
                        uses_result: [],
                    };
                },
                create(input) {
                    const expr = entity + ".create" + __callArgs(input || {});
                    return {
                        kind: "create",
                        qualified_entity: { entry_id, entity },
                        expr,
                        effect_class: "write",
                        result_shape: "mutation_result",
                        projection: [],
                        predicates: [],
                        input_bindings: __bindingsFromInput(input || {}),
                        depends_on: [],
                        uses_result: [],
                    };
                },
                ref(id) {
                    return {
                        action(name, input) {
                            const method = String(name);
                            const expr_template = entity + "(" + __quote(id) + ")." + method + __callArgs(input || {});
                            const input_bindings = __bindingsFromInput(input || {});
                            if (id && id.__bindingPath) input_bindings.push({ from: id.__bindingPath, to: "id" });
                            return {
                                kind: "action",
                                qualified_entity: { entry_id, entity },
                                expr_template,
                                effect_class: "side_effect",
                                result_shape: "side_effect_ack",
                                projection: [],
                                input_bindings,
                            };
                        },
                    };
                },
            };
        }

        export function forEach(source, fn) {
            const sourceId = __normalizeReturn(source);
            let binding = "item";
            if (typeof fn === "string") {
                binding = fn;
                fn = arguments[2];
            }
            const item = __symbol(binding);
            const effect = fn(item);
            return {
                __toPlanHandle(id) {
                    return this.as(id);
                },
                as(id) {
                    const node = {
                        id,
                        kind: "for_each",
                        effect_class: effect.effect_class,
                        result_shape: effect.result_shape,
                        source: sourceId,
                        item_binding: binding,
                        effect_template: {
                            kind: effect.kind,
                            qualified_entity: effect.qualified_entity,
                            expr_template: effect.expr_template || effect.expr,
                            effect_class: effect.effect_class,
                            result_shape: effect.result_shape,
                            projection: effect.projection || [],
                            input_bindings: effect.input_bindings || [],
                        },
                        depends_on: [sourceId],
                        uses_result: [{ node: sourceId, as: binding }],
                    };
                    return __nodeHandle(id, node);
                }
            };
        }

        export function derive(id, uses) {
            const uses_result = [];
            for (const [as, h] of Object.entries(uses || {})) {
                uses_result.push({ node: __normalizeReturn(h), as });
            }
            return __nodeHandle(id, {
                id,
                kind: "derive",
                effect_class: "artifact_read",
                result_shape: "artifact",
                depends_on: uses_result.map(u => u.node),
                uses_result,
            });
        }

        export function data(value) {
            return Plan.data(value);
        }

        export function parallel(...handles) {
            const nodes = [];
            for (const h of handles) __collectNodes(h, nodes);
            return { parallel: handles.map(__normalizeReturn), __planNodes: nodes };
        }
"#.to_string()
}

/// Bootstrap plus catalog-qualified `globalThis.plasm.<alias>.<Entity>` builders from a facade delta.
pub fn quickjs_runtime_from_facade_delta(delta: &FacadeDeltaV1) -> String {
    let mut out = quickjs_runtime_module_bootstrap();
    out.push_str("\nglobalThis.plasm = globalThis.plasm || {};\n");
    for q in &delta.qualified_entities {
        let alias = serde_json::Value::String(q.catalog_alias.clone()).to_string();
        let entity = serde_json::Value::String(q.entity.clone()).to_string();
        let entry_id = serde_json::Value::String(q.entry_id.clone()).to_string();
        let _ = writeln!(
            &mut out,
            "globalThis.plasm[{alias}] = globalThis.plasm[{alias}] || {{}};\n\
             globalThis.plasm[{alias}][{entity}] = makeEntity({entry_id}, {entity});"
        );
    }
    out
}

/// Smoke test: QuickJS can evaluate a bootstrap snippet.
#[cfg(test)]
mod tests {
    use super::{quickjs_runtime_from_facade_delta, quickjs_runtime_module_bootstrap};
    use crate::delta::FacadeDeltaV1;
    use rquickjs::{Context, Result as QjResult, Runtime};

    #[test]
    fn rquickjs_runs_bootstrap() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _v: () = ctx.eval(flat.as_str())?;
            let n: i32 = ctx.eval("1+1")?;
            assert_eq!(n, 2);
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn plasm_plan_json_collects_nodes() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js.replace("export function", "function").replace("export class", "class");
            let _v: () = ctx.eval(flat.as_str())?;
            let s: String = ctx
                .eval("const Product = makeEntity('acme', 'Product'); const n = __plasmBind(Product.query({}).select('id'), 'n1'); Plan.return(n)")
                .expect("plan");
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert_eq!(v["version"], 1);
            assert_eq!(v["kind"], "program");
            assert_eq!(v["nodes"][0]["qualified_entity"]["entry_id"], "acme");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn runtime_from_delta_installs_plasm_namespace() -> QjResult<()> {
        let delta = FacadeDeltaV1 {
            version: 1,
            catalog_entry_ids: vec!["acme".to_string()],
            catalog_aliases: vec![],
            qualified_entities: vec![crate::delta::QualifiedEntitySurface {
                entry_id: "acme".to_string(),
                catalog_alias: "acme".to_string(),
                entity: "Product".to_string(),
                description: None,
                e_index: Some(1),
                fields: vec![],
                relations: vec![],
                capabilities: vec![],
            }],
            collision_notes: vec![],
        };
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_from_facade_delta(&delta);
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let s: String = ctx.eval(
                "const n = __plasmBind(plasm.acme.Product.query({}), 'n1'); Plan.return(n)",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert_eq!(v["nodes"][0]["qualified_entity"]["entry_id"], "acme");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn plan_project_callback_lowers_indexed_field_path() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let s: String = ctx.eval(
                "const Product = makeEntity('acme', 'Product'); \
                 const src = __plasmBind(Product.query({}), 'src'); \
                 const p = __plasmBind(Plan.project(src, { name0: item => item.types[0].type.name }), 'p'); \
                 Plan.return(p)",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert_eq!(v["nodes"][1]["compute"]["op"]["fields"]["name0"][0], "types");
            assert_eq!(v["nodes"][1]["compute"]["op"]["fields"]["name0"][1], "0");
            assert_eq!(v["nodes"][1]["compute"]["op"]["fields"]["name0"][2], "type");
            assert_eq!(v["nodes"][1]["compute"]["op"]["fields"]["name0"][3], "name");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn plan_project_callback_rejects_array_methods_with_actionable_error() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let msg: String = ctx.eval(
                "try { \
                   const Product = makeEntity('acme', 'Product'); \
                   const src = __plasmBind(Product.query({}), 'src'); \
                   const p = __plasmBind(Plan.project(src, { names: item => item.types.map(x => x.type.name).join(',') }), 'p'); \
                   Plan.return(p); \
                   'NO_ERROR'; \
                 } catch (e) { String(e && e.message || e); }",
            )?;
            assert_ne!(msg, "NO_ERROR");
            assert!(msg.contains("unsupported array/string method") || msg.contains("Plan.project callbacks"), "{msg}");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }
}
