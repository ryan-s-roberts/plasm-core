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

        const __PLAN_BRANDS = "__plasmPlanBrands";
        const __BRAND_EFFECT = "PlanEffect";
        const __BRAND_SOURCE = "PlanSource";
        const __BRAND_BUILDER = "PlanBuilder";
        const __BRAND_HANDLE = "BoundPlanHandle";
        const __BRAND_VALUE = "PlanValueExpr";

        function __brand(target, ...brands) {
            if (!target || typeof target !== "object") return target;
            const current = Array.isArray(target[__PLAN_BRANDS]) ? target[__PLAN_BRANDS].slice() : [];
            for (const brand of brands) {
                if (!current.includes(brand)) current.push(brand);
            }
            Object.defineProperty(target, __PLAN_BRANDS, {
                value: current,
                enumerable: false,
                configurable: true,
            });
            return target;
        }

        function __hasBrand(v, brand) {
            return !!(v && typeof v === "object" && Array.isArray(v[__PLAN_BRANDS]) && v[__PLAN_BRANDS].includes(brand));
        }

        function __isPlanEffect(v) {
            return __hasBrand(v, __BRAND_EFFECT);
        }

        function __isPlanSource(v) {
            return __hasBrand(v, __BRAND_SOURCE);
        }

        function __planEffect(node, source) {
            return source ? __brand(node, __BRAND_EFFECT, __BRAND_SOURCE) : __brand(node, __BRAND_EFFECT);
        }

        function __planBuilder(builder, source) {
            return source ? __brand(builder, __BRAND_BUILDER, __BRAND_SOURCE) : __brand(builder, __BRAND_BUILDER);
        }

        function __planValueExpr(value) {
            return __brand(value, __BRAND_VALUE);
        }

        function __isSpecial(v) {
            return v && typeof v === "object" && (v.__plasmExpr || v.__planValue || v.__bindingPath || v.__planNodeId || v.__toPlanHandle || __isPlanEffect(v));
        }

        function __valueMeta(v) {
            if (v && v.__planValue) return v.__planValue;
            if (v && v.__bindingPath) return { kind: "binding_symbol", binding: v.__bindingName || String(v.__bindingPath).split(".")[0], path: v.__bindingFieldPath || [] };
            if (v && v.__planNodeId) return { kind: "symbol", path: v.__planNodeId };
            if (__isPlanEffect(v) && typeof v.expr === "string" && v.expr.includes("${")) return { kind: "template", template: v.expr, input_bindings: [] };
            if (__isPlanEffect(v) && typeof v.expr === "string") return { kind: "literal", value: v.expr };
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

        function __symbolString(path) {
            return "${" + path + "}";
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

        function __projectionFields(fields) {
            const out = fields.flat().map(String).map(f => f.trim()).filter(Boolean);
            if (out.length === 0) {
                throw new Error("Code Mode DSL error: select(...) requires at least one field name");
            }
            return out;
        }

        function __replaceProjection(expr, fields) {
            const base = String(expr || "").replace(/\[[^\]]*\]$/, "");
            return fields.length ? base + "[" + fields.join(",") + "]" : base;
        }

        function __projectedPlanNode(node, fields) {
            if (!node || typeof node.expr !== "string") {
                throw new Error("Code Mode DSL error: select(...) requires a Plasm read source with an expression");
            }
            const projection = __projectionFields(fields);
            return Object.assign({}, node, {
                expr: __replaceProjection(node.expr, projection),
                projection,
            });
        }

        function __projectPlanSource(source, fields) {
            if (__hasBrand(source, __BRAND_BUILDER) && typeof source.select === "function") {
                return source.select.apply(source, fields);
            }
            if (__isPlanEffect(source)) {
                const node = __projectedPlanNode(source, fields);
                node.__relations = source.__relations || [];
                const projected = __attachPlanSourceMethods(__planEffect(node, true), node.__relations);
                projected.__cardinality = source.__cardinality;
                return projected;
            }
            if (__hasBrand(source, __BRAND_HANDLE) && source.__planNodeId && source.__planNodes) {
                const nodes = source.__planNodes.slice();
                const last = nodes.length ? nodes[nodes.length - 1] : null;
                const node = __projectedPlanNode(last, fields);
                nodes[nodes.length - 1] = Object.assign({}, node, { id: source.__planNodeId });
                const handle = {
                    __planNodeId: source.__planNodeId,
                    __planNodes: nodes,
                    __relations: source.__relations || node.__relations || [],
                    __resultShape: node.result_shape || source.__resultShape || "list",
                    __cardinality: source.__cardinality,
                };
                return __nodeHandleProxy(__brand(handle, __BRAND_HANDLE, __BRAND_SOURCE));
            }
            throw new Error("Code Mode DSL error: select(...) is only valid on branded Plasm read sources such as query(...), search(...), get(...), or a bound Plan node");
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

        function __sourcePlan(source, childId) {
            const nodes = [];
            if (__hasBrand(source, __BRAND_HANDLE) && source.__planNodeId) {
                __collectNodes(source, nodes);
                return { sourceId: __normalizeReturn(source), nodes };
            }
            if (__isPlanEffect(source)) {
                const sourceId = source.id || String(childId) + "_source";
                nodes.push(Object.assign({}, source, { id: sourceId }));
                return { sourceId, nodes };
            }
            if (__hasBrand(source, __BRAND_BUILDER) && typeof source.yield === "function") {
                const sourceId = String(childId) + "_source";
                nodes.push(Object.assign({}, source.yield(), { id: sourceId }));
                return { sourceId, nodes };
            }
            if (__hasBrand(source, __BRAND_BUILDER) && source.__toPlanHandle) {
                const sourceId = String(childId) + "_source";
                const handle = source.__toPlanHandle(sourceId);
                __collectNodes(handle, nodes);
                return { sourceId: handle && handle.__planNodeId ? handle.__planNodeId : sourceId, nodes };
            }
            throw new Error("Plan source must be a branded PlanSource from a query/search/get/data/compute/map/relation handle");
        }

        function __dedupeNodes(nodes) {
            const seen = new Set();
            return nodes.filter(n => {
                if (!n || !n.id) return false;
                if (seen.has(n.id)) return false;
                seen.add(n.id);
                return true;
            });
        }

        function __returnNodeId(value, suggestedId, nodes) {
            if (__hasBrand(value, __BRAND_HANDLE) && value.__planNodeId) {
                __collectNodes(value, nodes);
                return value.__planNodeId;
            }
            if (__isPlanEffect(value) || __hasBrand(value, __BRAND_BUILDER)) {
                const id = suggestedId || __anonId();
                if (__isPlanEffect(value)) {
                    nodes.push(Object.assign({}, value, { id }));
                    return id;
                }
                if (value.__toPlanHandle) {
                    const handle = value.__toPlanHandle(id);
                    __collectNodes(handle, nodes);
                    return handle && handle.__planNodeId ? handle.__planNodeId : id;
                }
            }
            throw new Error("Plan.return values must be branded Plan nodes or buildable Plan sources");
        }

        function __returnPlan(value, nodes, suggestedId) {
            if (Array.isArray(value)) {
                if (value.length === 0) throw new Error("Plan.return parallel arrays must not be empty");
                return {
                    kind: "parallel",
                    nodes: value.map((item, i) => __returnNodeId(item, (suggestedId || "return") + "_" + (i + 1), nodes)),
                };
            }
            if (value && typeof value === "object" && !__isPlanSource(value) && !__isPlanEffect(value) && !__hasBrand(value, __BRAND_BUILDER)) {
                const fields = {};
                const keys = Object.keys(value);
                if (keys.length === 0) throw new Error("Plan.return record objects must not be empty");
                for (const key of keys) {
                    if (!String(key).trim()) throw new Error("Plan.return record names must be non-empty");
                    fields[key] = __returnNodeId(value[key], key, nodes);
                }
                return { kind: "record", fields };
            }
            return { kind: "node", node: __returnNodeId(value, suggestedId || "return", nodes) };
        }

        function __nodeHandle(id, node) {
            const nid = id || __anonId();
            const h = { __planNodeId: nid };
            if (node) {
                h.__planNodes = [Object.assign({}, node, { id: nid })];
                h.__relations = node.__relations || [];
                h.__resultShape = node.result_shape || "list";
            }
            return __nodeHandleProxy(__brand(h, __BRAND_HANDLE, __BRAND_SOURCE));
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
            if (__isPlanEffect(value)) return __nodeHandle(String(id), value);
            return value;
        }

        function __symbol(path) {
            const parts = String(path).split(".").filter(Boolean);
            const binding = parts.shift() || String(path);
            return new Proxy(__planValueExpr({ __bindingPath: path, __bindingName: binding, __bindingFieldPath: parts, __plasmExpr: "${" + path + "}" }), {
                get(target, prop) {
                    if (prop in target) return target[prop];
                    if (prop === Symbol.toPrimitive) return function() { return __symbolString(path); };
                    if (prop === "toString") return function() { return __symbolString(path); };
                    if (typeof prop === "symbol") return target[prop];
                    if (prop === "__planValue" || prop === "__planNodeId" || prop === "__toPlanHandle" || prop === "__planNodes") return undefined;
                    if (["map", "filter", "join", "reduce", "flatMap", "forEach"].includes(String(prop))) return __unsupportedProjectionMethod(String(prop));
                    return __symbol(path + "." + String(prop));
                }
            });
        }

        function __nodeHandleProxy(handle, cardinality) {
            if (cardinality) handle.__cardinality = cardinality;
            return new Proxy(handle, {
                get(target, prop) {
                    if (prop in target) return target[prop];
                    if (typeof prop === "symbol") return target[prop];
                    if (prop === "__planValue" || prop === "__toPlanHandle" || prop === "__cardinality" || prop === "toJSON") return undefined;
                    if (prop === "select") return function(...fields) { return __projectPlanSource(target, fields); };
                    const rel = (target.__relations || []).find(r => String(r.name) === String(prop));
                    if (rel) return function() { return __relationTraversal(target, rel); };
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
            const ref = __planValueExpr({
                __planValue: value,
                __planNodes: handle && handle.__planNodes ? handle.__planNodes : undefined,
                __plasmExpr: "${" + [node].concat(parts).join(".") + "}",
                __nodeInput: { node, alias: node, cardinality: cardinality || "auto" },
            });
            return new Proxy(ref, {
                get(target, prop) {
                    if (prop in target) return target[prop];
                    if (prop === Symbol.toPrimitive) return function() { return target.__plasmExpr; };
                    if (prop === "toString") return function() { return target.__plasmExpr; };
                    if (typeof prop === "symbol") return target[prop];
                    if (prop === "__bindingPath" || prop === "__bindingName" || prop === "__bindingFieldPath" || prop === "__planNodeId" || prop === "__toPlanHandle" || prop === "toJSON") return undefined;
                    return __nodeRef(handle, node, parts.concat(String(prop)).join("."), cardinality || "auto");
                }
            });
        }

        function __planSingleton(handle) {
            if (!__isPlanSource(handle)) throw new Error("Code Mode DSL error: Plan.singleton expects a branded Plan source");
            if (handle.__planNodeId) return __nodeHandleProxy(handle, "singleton");
            const source = Object.assign({}, handle);
            source.__cardinality = "singleton";
            return __brand(source, ...((handle && handle[__PLAN_BRANDS]) || []), __BRAND_SOURCE);
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
                return __planBuilder({
                    __toPlanHandle(id) {
                        return __nodeHandle(id, effect);
                    }
                }, true);
            }
            static data(value) {
                return __planBuilder({
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
                }, true);
            }
            static map(source, fn) {
                return Plan._map.apply(null, arguments);
            }
            static singleton(source) {
                return __planSingleton(source);
            }
            static _map(source, fn) {
                let binding = "item";
                if (typeof fn === "string") {
                    binding = fn;
                    fn = arguments[2];
                }
                const item = __symbol(binding);
                const value = fn(item);
                const valueMeta = __valueMeta(value);
                const inputs = __nodeInputsFromValueMeta(valueMeta);
                return __planBuilder({
                    __toPlanHandle(id) {
                        const sourcePlan = __sourcePlan(source, id);
                        const sourceId = sourcePlan.sourceId;
                        const nodes = sourcePlan.nodes.slice();
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
                }, true);
            }
            static project(source, spec) {
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
                return Plan._compute(source, { kind: "filter", predicates: predicates.flat() }, { entity: "PlanFilter", fields: [{ name: "value", value_kind: "unknown" }] });
            }
            static aggregate(source, aggregates) {
                return Plan._compute(source, { kind: "aggregate", aggregates: aggregates || [] }, __schemaFromFields("PlanAggregate", (aggregates || []).map(a => a.name), []));
            }
            static groupBy(source, keyFn) {
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
                const binding = "item";
                const key = __pathFromSymbol(keyFn(__symbol(binding)), binding);
                return Plan._compute(source, { kind: "sort", key, descending: direction === "desc" }, { entity: "PlanSort", fields: [{ name: "value", value_kind: "unknown" }] });
            }
            static limit(source, count) {
                return Plan._compute(source, { kind: "limit", count: Number(count) }, { entity: "PlanLimit", fields: [{ name: "value", value_kind: "unknown" }] });
            }
            static table(source, spec) {
                const columns = (spec && spec.columns) || [];
                return Plan._compute(source, { kind: "table_from_matrix", columns, has_header: !!(spec && spec.hasHeader) }, __schemaFromFields("PlanTable", columns, []));
            }
            static _compute(source, op, schema) {
                return __planBuilder({
                    __toPlanHandle(id) {
                        const sourcePlan = __sourcePlan(source, id);
                        const sourceId = sourcePlan.sourceId;
                        const nodes = sourcePlan.nodes.slice();
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
                }, true);
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
                this._return = { kind: "parallel", nodes: ids };
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
                this._return = __returnPlan(value, this.nodes, "return");
                this.nodes = __dedupeNodes(this.nodes);
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
            return __planValueExpr({
                __plasmExpr: JSON.stringify(String(days) + "d"),
                __planValue: { kind: "helper", name: "daysAgo", args: [days], display: String(days) + "d" },
            });
        }

        export function repo(owner, repo) {
            return { owner, repo };
        }

        export function linearTeam(id) {
            return String(id);
        }

        export function template(strings, ...values) {
            if (strings.length === 2 && strings[0] === "" && strings[1] === "" && values.length === 1) {
                const only = values[0];
                if (only && only.__planNodeId) {
                    return __nodeRef(only, String(only.__planNodeId), "", only.__cardinality || "auto");
                }
            }
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
                    } else if (v && v.__planNodeId) {
                        const node = String(v.__planNodeId);
                        raw += "${" + node + "}";
                        input_bindings.push({ from: node, to: "", node, alias: node, cardinality: v.__cardinality || "auto" });
                    } else {
                        raw += String(v);
                    }
                }
            }
            return __planValueExpr({
                __plasmExpr: "template(" + JSON.stringify(raw) + ")",
                __planValue: { kind: "template", template: raw, input_bindings },
                input_bindings,
            });
        }

        function __bindingsFromInput(input) {
            const out = [];
            function visit(v, path) {
                if (v && v.__bindingPath) out.push({ from: v.__bindingPath, to: path });
                if (v && v.__planValue && v.__planValue.kind === "node_symbol") {
                    const display = __displayPlanValue(v.__planValue);
                    out.push({ from: display, to: path, node: v.__planValue.node, alias: v.__planValue.alias, cardinality: v.__nodeInput ? v.__nodeInput.cardinality : "auto" });
                }
                if (v && v.__planValue && v.__planValue.kind === "template") {
                    for (const b of (v.__planValue.input_bindings || [])) out.push(Object.assign({}, b, { to: b.to || path }));
                }
                if (Array.isArray(v)) {
                    for (let i = 0; i < v.length; i++) visit(v[i], path ? path + "." + i : String(i));
                    return;
                }
                if (v && typeof v === "object" && !__isSpecial(v)) {
                    for (const [k, child] of Object.entries(v)) visit(child, path ? path + "." + k : k);
                }
            }
            for (const [k, v] of Object.entries(input || {})) {
                visit(v, k);
            }
            return out;
        }

        function __usesFromBindings(bindings) {
            const seen = {};
            const out = [];
            for (const b of (bindings || [])) {
                if (!b.node) continue;
                const node = String(b.node);
                const alias = String(b.alias || b.node);
                const key = node + "\u0000" + alias;
                if (seen[key]) continue;
                seen[key] = true;
                out.push({ node, as: alias });
            }
            return out;
        }

        function __callArgs(input) {
            const keys = Object.keys(input || {});
            if (keys.length === 0) return "";
            return "(" + keys.map(k => k + "=" + __quote(input[k])).join(", ") + ")";
        }

        function __capability(capabilities, kind) {
            return (capabilities || []).find(c => c.kind === kind) || {};
        }

        function __irHole(kind, data) {
            return { __plasm_hole: Object.assign({ kind }, data || {}) };
        }

        function __irValue(v) {
            if (v && v.__bindingPath) {
                return __irHole("binding", { binding: v.__bindingName || String(v.__bindingPath).split(".")[0], path: v.__bindingFieldPath || [] });
            }
            if (v && v.__planValue && v.__planValue.kind === "node_symbol") {
                return __irHole("node_input", { node: v.__planValue.node, alias: v.__planValue.alias || v.__planValue.node, path: v.__planValue.path || [], cardinality: v.__nodeInput ? v.__nodeInput.cardinality : "auto" });
            }
            if (Array.isArray(v)) return v.map(__irValue);
            if (v && typeof v === "object" && !__isSpecial(v)) {
                const out = {};
                for (const [k, child] of Object.entries(v)) out[k] = __irValue(child);
                return out;
            }
            return v == null ? null : v;
        }

        function __hasIrHole(v) {
            if (!v || typeof v !== "object") return false;
            if (v.__plasm_hole) return true;
            if (Array.isArray(v)) return v.some(__hasIrHole);
            return Object.values(v).some(__hasIrHole);
        }

        function __irPredicateFromPlan(p) {
            const op = { eq: "=", ne: "!=", lt: "<", lte: "<=", gt: ">", gte: ">=", contains: "contains", in: "in", exists: "exists" }[p.op] || "=";
            return { type: "comparison", field: (p.field_path || []).join("."), op, value: __irValue((p.value || {}).value) };
        }

        function __irPredicates(filters, predicates) {
            const out = [];
            for (const [k, v] of Object.entries(filters || {})) {
                out.push({ type: "comparison", field: k, op: "=", value: __irValue(v) });
            }
            for (const p of (predicates || [])) out.push(__irPredicateFromPlan(p));
            if (out.length === 0) return null;
            if (out.length === 1) return out[0];
            return { type: "and", args: out };
        }

        function __planIr(expr, projection, displayExpr) {
            return { expr, projection: projection && projection.length ? projection : undefined, display_expr: displayExpr };
        }

        const __plasmRelations = {};

        function __relationsFor(entry_id, entity) {
            return (((__plasmRelations || {})[entry_id] || {})[entity] || []);
        }

        function __searchInput(input, searchParam) {
            if (input && typeof input === "object" && !Array.isArray(input) && !__isSpecial(input)) {
                const candidates = [searchParam, "q", "query", "search", "text"].filter(Boolean).map(String);
                const textKey = candidates.find(k => Object.prototype.hasOwnProperty.call(input, k));
                const text = textKey ? input[textKey] : "";
                const filters = {};
                for (const k of Object.keys(input)) {
                    if (k !== textKey) filters[k] = input[k];
                }
                return { text, filters };
            }
            return { text: input, filters: {} };
        }

        function __surfaceBuilder(entry_id, entity, kind, input, relations, searchParam, capabilities) {
            let projection = [];
            let baseFilters = {};
            let searchText = null;
            if (kind === "search") {
                const parsed = __searchInput(input, searchParam);
                searchText = parsed.text;
                baseFilters = parsed.filters || {};
            } else {
                baseFilters = input || {};
            }
            let predicates = __filterPredicates(baseFilters);
            const builder = {
                where(...ps) {
                    predicates = predicates.concat(ps.flat());
                    return this;
                },
                select(...fields) {
                    projection = fields;
                    return this;
                },
                yield() {
                    const extraPredicates = predicates.slice(Object.keys(baseFilters || {}).length);
                    const exprBase = kind === "search"
                        ? entity + "~" + __quote(searchText) + __filters(baseFilters || {}, extraPredicates)
                        : entity + __filters(baseFilters || {}, extraPredicates);
                    const expr = projection.length ? exprBase + "[" + projection.join(",") + "]" : exprBase;
                    const cap = __capability(capabilities, kind);
                    const irFilters = Object.assign({}, baseFilters || {});
                    if (kind === "search") irFilters[searchParam || "q"] = searchText;
                    const predicate = __irPredicates(irFilters, extraPredicates);
                    const irExpr = {
                        op: "query",
                        entity,
                        predicate: predicate || undefined,
                        projection: projection.length ? projection : undefined,
                        capability_name: cap.name || undefined,
                    };
                    const ir = __planIr(irExpr, projection, expr);
                    const node = {
                        kind,
                        qualified_entity: { entry_id, entity },
                        expr,
                        ir,
                        effect_class: "read",
                        result_shape: "list",
                        projection,
                        predicates,
                        depends_on: [],
                        uses_result: [],
                    };
                    node.__relations = relations || [];
                    return __attachPlanSourceMethods(__planEffect(node, true), relations || []);
                },
                as(id) {
                    return __nodeHandle(id, Object.assign(this.yield(), { id }));
                },
                __toPlanHandle(id) {
                    return __nodeHandle(id, this.yield());
                },
            };
            return __attachPlanSourceMethods(__planBuilder(builder, true), relations || []);
        }

        function __attachRelationMethods(target, relations) {
            for (const rel of (relations || [])) {
                const name = String(rel.name);
                if (target[name]) continue;
                target[name] = function() {
                    return __relationTraversal(target, rel);
                };
            }
            target.__relations = relations || [];
            return target;
        }

        function __attachPlanSourceMethods(target, relations) {
            if (!target.select) {
                target.select = function(...fields) {
                    return __projectPlanSource(target, fields);
                };
            }
            return __attachRelationMethods(target, relations || []);
        }

        function __relationTraversal(source, relation) {
            return __planBuilder({
                __toPlanHandle(id) {
                    return this.as(id);
                },
                as(id) {
                    const sourcePlan = __sourcePlan(source, id);
                    const sourceId = sourcePlan.sourceId;
                    const nodes = sourcePlan.nodes.slice();
                    const sourceNode = nodes[nodes.length - 1];
                    if (!sourceNode || !sourceNode.ir) {
                        throw new Error("Relation traversal requires a source node with Plasm IR");
                    }
                    const expr = sourceNode.expr + "." + relation.name;
                    const ir = __planIr({ op: "chain", source: sourceNode.ir.expr, selector: relation.name, step: { type: "auto_get" } }, [], expr);
                    const sourceCardinality = source.__cardinality === "singleton"
                        ? "runtime_checked_singleton"
                        : (sourceNode.result_shape === "single" ? "single" : "many");
                    const resultShape = relation.cardinality === "one" && sourceCardinality !== "many" ? "single" : "list";
                    const target = { entry_id: relation.entry_id, entity: relation.target };
                    const node = {
                        id,
                        kind: "relation",
                        effect_class: "read",
                        result_shape: resultShape,
                        relation: {
                            source: sourceId,
                            relation: relation.name,
                            target,
                            cardinality: relation.cardinality,
                            source_cardinality: sourceCardinality,
                            expr,
                            ir,
                        },
                        qualified_entity: target,
                        projection: [],
                        predicates: [],
                        depends_on: [sourceId],
                        uses_result: [{ node: sourceId, as: "source" }],
                    };
                    node.__relations = __relationsFor(relation.entry_id, relation.target);
                    nodes.push(node);
                    const handle = __nodeHandle(id, node);
                    handle.__planNodes = nodes;
                    return handle;
                }
            }, true);
        }

        export function makeEntity(entry_id, entity, relations, searchParam, capabilities) {
            return {
                query(filters) {
                    return __surfaceBuilder(entry_id, entity, "query", filters, relations || [], null, capabilities || []);
                },
                search(input) {
                    return __surfaceBuilder(entry_id, entity, "search", input, relations || [], searchParam || "q", capabilities || []);
                },
                get(id) {
                    const display = entity + "(" + __quote(id) + ")";
                    const ir = __planIr({ op: "get", ref: { entity_type: entity, key: String(id) } }, [], display);
                    const node = {
                        kind: "get",
                        qualified_entity: { entry_id, entity },
                        expr: display,
                        ir,
                        effect_class: "read",
                        result_shape: "single",
                        projection: [],
                        predicates: [],
                        depends_on: [],
                        uses_result: [],
                    };
                    node.__relations = relations || [];
                    return __attachPlanSourceMethods(__planEffect(node, true), relations || []);
                },
                create(input) {
                    const expr = entity + ".create" + __callArgs(input || {});
                    const input_bindings = __bindingsFromInput(input || {});
                    const uses_result = __usesFromBindings(input_bindings);
                    const cap = __capability(capabilities || [], "create");
                    const irExpr = { op: "create", capability: cap.name || (entity.toLowerCase() + "_create"), entity, input: __irValue(input || {}) };
                    const irContract = __planIr(irExpr, [], expr);
                    const templated = __hasIrHole(irExpr);
                    return __planEffect({
                        kind: "create",
                        qualified_entity: { entry_id, entity },
                        expr,
                        ir: templated ? undefined : irContract,
                        ir_template: templated ? Object.assign({ input_bindings }, irContract) : undefined,
                        effect_class: "write",
                        result_shape: "mutation_result",
                        projection: [],
                        predicates: [],
                        input_bindings,
                        depends_on: uses_result.map(u => u.node),
                        uses_result,
                    }, false);
                },
                ref(id) {
                    return {
                        action(name, input) {
                            const method = String(name);
                            const expr_template = entity + "(" + __quote(id) + ")." + method + __callArgs(input || {});
                            const input_bindings = __bindingsFromInput(input || {});
                            if (id && id.__bindingPath) input_bindings.push({ from: id.__bindingPath, to: "id" });
                            const uses_result = __usesFromBindings(input_bindings);
                            const targetKey = __irValue(id);
                            const irExpr = {
                                op: "invoke",
                                capability: method,
                                target: { entity_type: entity, key: targetKey },
                                input: input == null ? undefined : __irValue(input),
                            };
                            const irContract = __planIr(irExpr, [], expr_template);
                            const templated = __hasIrHole(irExpr);
                            return __planEffect({
                                kind: "action",
                                qualified_entity: { entry_id, entity },
                                expr_template,
                                ir: templated ? undefined : irContract,
                                ir_template: templated ? Object.assign({ input_bindings }, irContract) : undefined,
                                effect_class: "side_effect",
                                result_shape: "side_effect_ack",
                                projection: [],
                                input_bindings,
                                depends_on: uses_result.map(u => u.node),
                                uses_result,
                            }, false);
                        },
                    };
                },
            };
        }

        export function forEach(source, fn) {
            let binding = "item";
            if (typeof fn === "string") {
                binding = fn;
                fn = arguments[2];
            }
            const item = __symbol(binding);
            const effect = fn(item);
            return __planBuilder({
                __toPlanHandle(id) {
                    return this.as(id);
                },
                as(id) {
                    const sourcePlan = __sourcePlan(source, id);
                    const sourceId = sourcePlan.sourceId;
                    const nodes = sourcePlan.nodes.slice();
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
                            ir_template: effect.ir_template || effect.ir,
                            effect_class: effect.effect_class,
                            result_shape: effect.result_shape,
                            projection: effect.projection || [],
                            input_bindings: effect.input_bindings || [],
                        },
                        depends_on: [sourceId],
                        uses_result: [{ node: sourceId, as: binding }],
                    };
                    nodes.push(node);
                    const handle = __nodeHandle(id, node);
                    handle.__planNodes = nodes;
                    return handle;
                }
            }, false);
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
        let relations = q
            .relations
            .iter()
            .map(|r| {
                serde_json::json!({
                    "name": r.name,
                    "target": r.target,
                    "cardinality": r.cardinality,
                    "entry_id": q.entry_id,
                })
            })
            .collect::<Vec<_>>();
        let relations_json = serde_json::to_string(&relations).unwrap_or_else(|_| "[]".to_string());
        let search_param = q
            .capabilities
            .iter()
            .find(|c| c.kind == "search")
            .and_then(|cap| {
                cap.input_parameters
                    .iter()
                    .find(|p| p.role.as_deref() == Some("search"))
                    .or_else(|| cap.input_parameters.iter().find(|p| p.required))
                    .map(|p| p.name.clone())
            })
            .unwrap_or_else(|| "q".to_string());
        let search_param = serde_json::Value::String(search_param).to_string();
        let capabilities_json =
            serde_json::to_string(&q.capabilities).unwrap_or_else(|_| "[]".to_string());
        let _ = writeln!(
            &mut out,
            "globalThis.plasm[{alias}] = globalThis.plasm[{alias}] || {{}};\n\
             __plasmRelations[{entry_id}] = __plasmRelations[{entry_id}] || {{}};\n\
             __plasmRelations[{entry_id}][{entity}] = {relations_json};\n\
             globalThis.plasm[{alias}][{entity}] = makeEntity({entry_id}, {entity}, {relations_json}, {search_param}, {capabilities_json});"
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
            assert_eq!(v["return"]["kind"], "node");
            assert_eq!(v["return"]["node"], "n1");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn plasm_plan_get_select_projects_read_source() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let s: String = ctx.eval(
                "const Repository = makeEntity('github', 'Repository'); \
                 Plan.return(Repository.get('joshrieken/plasm').select('full_name', 'pushed_at'))",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert_eq!(v["nodes"][0]["kind"], "get");
            assert_eq!(
                v["nodes"][0]["expr"],
                "Repository(\"joshrieken/plasm\")[full_name,pushed_at]"
            );
            assert_eq!(v["nodes"][0]["projection"][0], "full_name");
            assert_eq!(v["return"]["kind"], "node");
            assert_eq!(v["return"]["node"], "return");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn plasm_plan_singleton_accepts_projected_get_source() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let s: String = ctx.eval(
                "const Repository = makeEntity('github', 'Repository'); \
                 Plan.return(Plan.singleton(Repository.get('joshrieken/plasm').select('full_name')))",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert_eq!(v["nodes"][0]["kind"], "get");
            assert_eq!(
                v["nodes"][0]["expr"],
                "Repository(\"joshrieken/plasm\")[full_name]"
            );
            assert_eq!(v["nodes"][0]["projection"][0], "full_name");
            assert_eq!(v["return"]["node"], "return");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn plasm_plan_return_emits_tagged_record_and_parallel() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js.replace("export function", "function").replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let record: String = ctx.eval(
                "const Product = makeEntity('acme', 'Product'); \
                 Plan.return({ sorted: Plan.sort(Product.query({}), row => row.name), detail: Product.get('p1') })",
            )?;
            let v: serde_json::Value = serde_json::from_str(&record).expect("json");
            assert_eq!(v["return"]["kind"], "record");
            assert_eq!(v["return"]["fields"]["sorted"], "sorted");
            assert_eq!(v["return"]["fields"]["detail"], "detail");
            assert_eq!(v["nodes"][0]["id"], "sorted_source");
            assert_eq!(v["nodes"][2]["id"], "detail");

            let parallel: String = ctx.eval(
                "const Product2 = makeEntity('acme', 'Product'); \
                 const a = __plasmBind(Product2.get('a'), 'a'); \
                 const b = __plasmBind(Product2.get('b'), 'b'); \
                 Plan.return([a, b])",
            )?;
            let p: serde_json::Value = serde_json::from_str(&parallel).expect("json");
            assert_eq!(p["return"]["kind"], "parallel");
            assert_eq!(p["return"]["nodes"][0], "a");
            assert_eq!(p["return"]["nodes"][1], "b");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn plasm_plan_return_rejects_arbitrary_nested_maps() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js.replace("export function", "function").replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let msg: String = ctx.eval(
                "let msg = ''; \
                 try { Plan.return({ invalid: { node: 'n1' } }); } catch (e) { msg = String(e && e.message || e); } \
                 msg",
            )?;
            assert!(msg.contains("Plan.return values must be branded"), "{msg}");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn plasm_plan_json_serializes_search_nodes() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js.replace("export function", "function").replace("export class", "class");
            let _v: () = ctx.eval(flat.as_str())?;
            let s: String = ctx
                .eval("const Product = makeEntity('acme', 'Product', [], 'term'); const n = __plasmBind(Product.search({term: 'bolt', active: true}).select('id'), 'n1'); Plan.return(n)")
                .expect("plan");
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert_eq!(v["nodes"][0]["kind"], "search");
            assert_eq!(v["nodes"][0]["expr"], "Product~\"bolt\"{active=true}[id]");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn plasm_plan_json_serializes_relation_nodes() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js.replace("export function", "function").replace("export class", "class");
            let _v: () = ctx.eval(flat.as_str())?;
            let s: String = ctx
                .eval("const rels = [{ name: 'category', target: 'Category', cardinality: 'one', entry_id: 'acme' }]; const Product = makeEntity('acme', 'Product', rels); const p = __plasmBind(Product.get('p1'), 'p'); const c = __plasmBind(p.category(), 'c'); Plan.return(c)")
                .expect("plan");
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert_eq!(v["nodes"][1]["kind"], "relation");
            assert_eq!(v["nodes"][1]["relation"]["relation"], "category");
            assert_eq!(v["nodes"][1]["relation"]["expr"], "Product(\"p1\").category");
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
    fn plan_limit_materializes_unbound_query_source() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let s: String = ctx.eval(
                "const Pokemon = makeEntity('acme', 'Pokemon'); \
                 const limited = __plasmBind(Plan.limit(Pokemon.query({}), 3), 'p'); \
                 Plan.return(limited)",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert_eq!(v["nodes"][0]["id"], "p_source");
            assert_eq!(v["nodes"][0]["kind"], "query");
            assert_eq!(v["nodes"][1]["compute"]["source"], "p_source");
            assert_eq!(v["nodes"][1]["compute"]["op"]["kind"], "limit");
            assert_eq!(v["nodes"][1]["depends_on"][0], "p_source");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn plan_sources_are_branded_and_unbranded_sources_rejected() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let report: String = ctx.eval(
                "let out; \
                 try { \
                   const Pokemon = makeEntity('acme', 'Pokemon'); \
                   const q = Pokemon.query({}); \
                   const h = __plasmBind(q, 'q'); \
                   let msg = ''; \
                   try { __plasmBind(Plan.limit({ __planNodeId: 'raw' }, 1), 'bad'); } catch (e) { msg = String(e && e.message || e); } \
                   out = { \
                     querySource: Array.isArray(q.__plasmPlanBrands) && q.__plasmPlanBrands.includes('PlanSource'), \
                     handleBrand: Array.isArray(h.__plasmPlanBrands) && h.__plasmPlanBrands.includes('BoundPlanHandle'), \
                     hiddenInJson: !JSON.stringify(h).includes('__plasmPlanBrands'), \
                     rejected: msg.includes('branded PlanSource'), \
                     msg \
                   }; \
                 } catch (e) { out = { fatal: String(e && e.message || e) }; } \
                 JSON.stringify(out)",
            )?;
            let v: serde_json::Value = serde_json::from_str(&report).expect("json");
            assert!(v.get("fatal").is_none(), "{v}");
            assert_eq!(v["querySource"], true);
            assert_eq!(v["handleBrand"], true);
            assert_eq!(v["hiddenInJson"], true);
            assert_eq!(v["rejected"], true);
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn plan_project_materializes_unbound_search_source() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let s: String = ctx.eval(
                "const Pokemon = makeEntity('acme', 'Pokemon'); \
                 const projected = __plasmBind(Plan.project(Pokemon.search({ q: 'pikachu' }), { name: row => row.name }), 'p'); \
                 Plan.return(projected)",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert_eq!(v["nodes"][0]["id"], "p_source");
            assert_eq!(v["nodes"][0]["kind"], "search");
            assert_eq!(v["nodes"][1]["compute"]["source"], "p_source");
            assert_eq!(v["nodes"][1]["compute"]["op"]["kind"], "project");
            assert_eq!(v["nodes"][1]["compute"]["op"]["fields"]["name"][0], "name");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn plan_map_preserves_symbolic_string_get_template() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let s: String = ctx.eval(
                "const Pokemon = makeEntity('acme', 'Pokemon'); \
                 const Item = makeEntity('acme', 'Item'); \
                 const src = __plasmBind(Pokemon.query({}), 'src'); \
                 const mapped = __plasmBind(Plan.map(src, row => Item.get(String(row.id))), 'm'); \
                 Plan.return(mapped)",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            let template = v["nodes"][1]["derive_template"]["value"]["template"]
                .as_str()
                .expect("template");
            assert!(template.contains("${item.id}"), "{template}");
            assert!(!template.contains("[object Object]"), "{template}");
            assert_eq!(
                v["nodes"][1]["derive_template"]["value"]["kind"],
                "template"
            );
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn bare_template_of_singleton_node_supports_field_access() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let s: String = ctx.eval(
                "let out; \
                 try { \
                   const Team = makeEntity('acme', 'Team'); \
                   const Issue = makeEntity('acme', 'Issue'); \
                   const firstTeam = __plasmBind(Plan.singleton(Plan.limit(Team.query({}), 1)), 'firstTeam'); \
                   const issue = __plasmBind(Issue.create({ team: { api: 'linear', entity: 'Team', key: template`${firstTeam}`.id } }), 'issue'); \
                   out = JSON.parse(Plan.return(issue)); \
                 } catch (e) { out = { fatal: String(e && e.message || e) }; } \
                 JSON.stringify(out)",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert!(v.get("fatal").is_none(), "{v}");
            let create_node = v["nodes"]
                .as_array()
                .and_then(|nodes| {
                    nodes
                        .iter()
                        .find(|node| node["kind"].as_str() == Some("create"))
                })
                .unwrap_or_else(|| panic!("create node missing: {v}"));
            let expr = create_node["expr"]
                .as_str()
                .unwrap_or_else(|| panic!("create expr missing: {v}"));
            assert!(expr.contains("${firstTeam.id}"), "{expr}");
            assert!(!expr.contains("[object Object]"), "{expr}");
            assert_eq!(create_node["depends_on"][0], "firstTeam");
            assert_eq!(create_node["uses_result"][0]["node"], "firstTeam");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn for_each_materializes_unbound_query_source() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let s: String = ctx.eval(
                "const Pokemon = makeEntity('acme', 'Pokemon'); \
                 const Item = makeEntity('acme', 'Item'); \
                 const fx = __plasmBind(forEach(Pokemon.query({}), row => Item.ref(String(row.id)).action('sync', { name: row.name })), 'fx'); \
                 Plan.return(fx)",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert_eq!(v["nodes"][0]["id"], "fx_source");
            assert_eq!(v["nodes"][0]["kind"], "query");
            assert_eq!(v["nodes"][1]["source"], "fx_source");
            assert_eq!(v["nodes"][1]["depends_on"][0], "fx_source");
            let template = v["nodes"][1]["effect_template"]["expr_template"]
                .as_str()
                .expect("template");
            assert!(template.contains("${item.id}"), "{template}");
            assert!(!template.contains("[object Object]"), "{template}");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn relation_materializes_unbound_builder_source() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let s: String = ctx.eval(
                "const Product = makeEntity('acme', 'Product', [{ name: 'category', entry_id: 'acme', target: 'Category', cardinality: 'one' }]); \
                 const category = __plasmBind(Product.query({}).category(), 'cat'); \
                 Plan.return(category)",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert_eq!(v["nodes"][0]["id"], "cat_source");
            assert_eq!(v["nodes"][0]["kind"], "query");
            assert_eq!(v["nodes"][1]["kind"], "relation");
            assert_eq!(v["nodes"][1]["relation"]["source"], "cat_source");
            assert_eq!(v["nodes"][1]["depends_on"][0], "cat_source");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn relation_preserves_get_and_singleton_source_cardinality() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let s: String = ctx.eval(
                "const Product = makeEntity('acme', 'Product', [{ name: 'category', entry_id: 'acme', target: 'Category', cardinality: 'one' }]); \
                 const fromGet = Product.get('p1').category(); \
                 const getNode = __plasmBind(Product.get('p2'), 'p'); \
                 const fromSingleton = Plan.singleton(getNode).category(); \
                 Plan.return({ fromGet, fromSingleton })",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert_eq!(v["nodes"][1]["relation"]["source_cardinality"], "single");
            assert_eq!(
                v["nodes"][3]["relation"]["source_cardinality"],
                "runtime_checked_singleton"
            );
            assert_eq!(v["nodes"][1]["result_shape"], "single");
            assert_eq!(v["nodes"][3]["result_shape"], "single");
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
