//! Small JavaScript module (ESM shape) for QuickJS — generated with **genco** to keep the dependency manifest.

use std::fmt::Write as _;

use crate::delta::FacadeDeltaV1;

/// QuickJS helpers for building one **Plan** artifact (no host I/O).
pub fn quickjs_runtime_module_bootstrap() -> String {
    r#"
        export function entityRef(api, entity, key) {
            return __planValueExpr({ __plasmEntityRef: true, api, entity, key });
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

        function __attachRefMetadata(target, ref) {
            if (!target || typeof target !== "object" || !ref) return target;
            Object.defineProperty(target, "__plasmRef", {
                value: ref,
                enumerable: false,
                configurable: true,
            });
            return target;
        }

        function __isSpecial(v) {
            return v && typeof v === "object" && (v.__plasmExpr || v.__planValue || v.__bindingPath || v.__planNodeId || v.__toPlanHandle || __isPlanEffect(v) || v.__plasmEntityRef);
        }

        function __isEntityRefValue(v) {
            return !!(v && typeof v === "object" && v.__plasmEntityRef === true);
        }

        function __valueMeta(v) {
            if (v && v.__planValue) return v.__planValue;
            if (v && v.__bindingPath) return { kind: "binding_symbol", binding: v.__bindingName || String(v.__bindingPath).split(".")[0], path: v.__bindingFieldPath || [] };
            if (v && v.__planNodeId) return { kind: "symbol", path: v.__planNodeId };
            if (__isPlanEffect(v) && typeof v.expr === "string" && v.expr.includes("${")) return { kind: "template", template: v.expr, input_bindings: [] };
            if (__isPlanEffect(v) && typeof v.expr === "string") return { kind: "literal", value: v.expr };
            if (Array.isArray(v)) return { kind: "array", items: v.map(__valueMeta) };
            if (__isEntityRefValue(v)) return { kind: "entity_ref_key", api: String(v.api), entity: String(v.entity), key: __valueMeta(v.key) };
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
            if (__isEntityRefValue(v)) return __quote(v.key);
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
            if (node && node.relation && node.relation.ir) {
                const projection = __projectionFields(fields);
                const relation = Object.assign({}, node.relation, {
                    expr: __replaceProjection(node.relation.expr, projection),
                    ir: Object.assign({}, node.relation.ir, { projection }),
                });
                return Object.assign({}, node, {
                    relation,
                    projection,
                });
            }
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
            if (v.kind === "entity_ref_key") return __quoteFromPlanValue(v.key);
            if (v.kind === "array") return "[" + (v.items || []).map(__quoteFromPlanValue).join(",") + "]";
            if (v.kind === "object") {
                const fields = v.fields || {};
                return "{" + Object.keys(fields).map(k => k + "=" + __quoteFromPlanValue(fields[k])).join(", ") + "}";
            }
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
                throw new Error("Code Mode DSL error: Plan.return expects a single Plan node/effect or an array of Plan nodes/effects; object maps are not supported");
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
                if (node.__plasmRef) __attachRefMetadata(h, node.__plasmRef);
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
                    if (prop === "__planValue" || prop === "__toPlanHandle" || prop === "__cardinality" || prop === "__plasmRef" || prop === "toJSON") return undefined;
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
                    if (prop === "__bindingPath" || prop === "__bindingName" || prop === "__bindingFieldPath" || prop === "__planNodeId" || prop === "__toPlanHandle" || prop === "__plasmRef" || prop === "toJSON") return undefined;
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
                if (__isPlanSource(value)) {
                    return __planBuilder({
                        __toPlanHandle(id) {
                            const sourcePlan = __sourcePlan(source, id);
                            const sourceId = sourcePlan.sourceId;
                            const nodes = sourcePlan.nodes.slice();
                            const staged = __planSourceTemplateNode(value);
                            nodes.push({
                                id,
                                kind: "for_each",
                                effect_class: staged.effect_class,
                                result_shape: staged.result_shape || "list",
                                source: sourceId,
                                item_binding: binding,
                                effect_template: staged.effect_template,
                                projection: staged.projection || [],
                                predicates: staged.predicates || [],
                                depends_on: [sourceId].concat((staged.uses_result || []).map(u => u.node)),
                                uses_result: [{ node: sourceId, as: binding }].concat(staged.uses_result || []),
                            });
                            const handle = __nodeHandle(id, nodes[nodes.length - 1]);
                            handle.__planNodes = nodes;
                            return handle;
                        }
                    }, true);
                }
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
                const specs = __normalizeAggregates(aggregates);
                return Plan._compute(source, { kind: "aggregate", aggregates: specs }, __schemaFromFields("PlanAggregate", specs.map(a => a.name), []));
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
                        const specs = __normalizeAggregates(aggregates);
                        return Plan._compute(source, { kind: "group_by", key, aggregates: specs }, __schemaFromFields("PlanGroup", ["key"].concat(specs.map(a => a.name)), [key]));
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
                exists: () => ({ field_path: path, op: "exists", value: { kind: "literal", value: null } }),
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
                const symbolic = __symbolicStringParts(v);
                if (symbolic) out.push({ from: symbolic.binding + (symbolic.path.length ? "." + symbolic.path.join(".") : ""), to: path });
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
                if (__isEntityRefValue(v)) {
                    visit(v.key, path);
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

        function __planSourceTemplateNode(source) {
            const node = source && source.yield ? source.yield() : source;
            if (!node || !node.kind || !node.qualified_entity) {
                throw new Error("Plan.map callback returned a Plasm source that could not be staged");
            }
            const irTemplate = node.ir_template || node.ir;
            if (!irTemplate) {
                throw new Error("Plan.map callback returned a Plasm source without executable IR");
            }
            return {
                effect_class: node.effect_class || "read",
                result_shape: node.result_shape || "list",
                projection: node.projection || [],
                predicates: node.predicates || [],
                uses_result: node.uses_result || [],
                effect_template: {
                    kind: node.kind,
                    qualified_entity: node.qualified_entity,
                    expr_template: node.expr_template || node.expr || (irTemplate.display_expr || "<ir>"),
                    ir_template: Object.assign({
                        input_bindings: node.input_bindings || (node.ir_template && node.ir_template.input_bindings) || [],
                    }, irTemplate),
                    effect_class: node.effect_class || "read",
                    result_shape: node.result_shape || "list",
                    projection: node.projection || [],
                    predicates: node.predicates || [],
                    input_bindings: node.input_bindings || (node.ir_template && node.ir_template.input_bindings) || [],
                },
            };
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

        function __isIrStringSlot(v) {
            return typeof v === "string" || (v && typeof v === "object" && v.__plasm_hole);
        }

        function __symbolicStringParts(v) {
            if (typeof v !== "string") return null;
            const m = /^\$\{([A-Za-z_$][A-Za-z0-9_$]*)(?:\.([^}]+))?\}$/.exec(v);
            if (!m) return null;
            return { binding: m[1], path: m[2] ? m[2].split(".").filter(Boolean) : [] };
        }

        function __irValue(v) {
            if (v && v.__bindingPath) {
                return __irHole("binding", { binding: v.__bindingName || String(v.__bindingPath).split(".")[0], path: v.__bindingFieldPath || [] });
            }
            const symbolic = __symbolicStringParts(v);
            if (symbolic) return __irHole("binding", symbolic);
            if (v && v.__planValue && v.__planValue.kind === "node_symbol") {
                return __irHole("node_input", { node: v.__planValue.node, alias: v.__planValue.alias || v.__planValue.node, path: v.__planValue.path || [], cardinality: v.__nodeInput ? v.__nodeInput.cardinality : "auto" });
            }
            if (Array.isArray(v)) return v.map(__irValue);
            if (__isEntityRefValue(v)) return __irValue(v.key);
            if (v && typeof v === "object" && !__isSpecial(v)) {
                const out = {};
                for (const [k, child] of Object.entries(v)) out[k] = __irValue(child);
                return out;
            }
            return v == null ? null : v;
        }

        function __entityKeyValue(entity, keyVars, id) {
            const keys = Array.isArray(keyVars) ? keyVars : [];
            if (keys.length <= 1) {
                if (id && typeof id === "object" && !__isSpecial(id) && !Array.isArray(id)) {
                    throw new Error("Code Mode DSL error: " + entity + ".get(...) has a single-key identity and requires a string key, not an object");
                }
                const value = __irValue(id);
                if (!__isIrStringSlot(value)) {
                    throw new Error("Code Mode DSL error: " + entity + ".get(...) key must lower to a string");
                }
                return value;
            }
            if (typeof id === "string") {
                if (keys.length !== 2) {
                    throw new Error("Code Mode DSL error: " + entity + ".get(...) string shorthand is only supported for two-part compound keys; use an object with fields [" + keys.join(", ") + "]");
                }
                const idx = id.indexOf("/");
                if (idx < 0) {
                    throw new Error("Code Mode DSL error: " + entity + ".get(...) compound string shorthand must be 'left/right' for key_vars [" + keys.join(", ") + "]");
                }
                const left = id.slice(0, idx).trim();
                const right = id.slice(idx + 1).trim();
                if (!left || !right) {
                    throw new Error("Code Mode DSL error: " + entity + ".get(...) compound string shorthand must contain non-empty key parts for [" + keys.join(", ") + "]");
                }
                return { [keys[0]]: left, [keys[1]]: right };
            }
            if (!id || typeof id !== "object" || Array.isArray(id) || __isSpecial(id)) {
                throw new Error("Code Mode DSL error: " + entity + ".get(...) has compound key_vars [" + keys.join(", ") + "] and requires an object with exactly those string fields");
            }
            const got = Object.keys(id).sort();
            const expected = keys.slice().sort();
            if (got.length !== expected.length || got.some((k, i) => k !== expected[i])) {
                throw new Error("Code Mode DSL error: " + entity + ".get(...) requires compound key fields [" + keys.join(", ") + "]; got [" + got.join(", ") + "]");
            }
            const out = {};
            for (const key of keys) {
                const value = __irValue(id[key]);
                if (!__isIrStringSlot(value)) {
                    throw new Error("Code Mode DSL error: " + entity + ".get(...) compound key field '" + key + "' must lower to a string");
                }
                out[key] = value;
            }
            return out;
        }

        function __displayEntityKey(entity, keyVars, key) {
            const keys = Array.isArray(keyVars) ? keyVars : [];
            if (keys.length <= 1) return __quote(key);
            return "{" + keys.map(k => k + "=" + __quote(key[k])).join(", ") + "}";
        }

        function __irValueFromPlanValue(v) {
            if (!v || v.kind === "literal") return __irValue(v ? v.value : null);
            if (v.kind === "entity_ref_key") return __irValueFromPlanValue(v.key);
            if (v.kind === "binding_symbol") return __irHole("binding", { binding: v.binding, path: v.path || [] });
            if (v.kind === "node_symbol") return __irHole("node_input", { node: v.node, alias: v.alias || v.node, path: v.path || [], cardinality: "auto" });
            if (v.kind === "template") return __irValue(v.template);
            if (v.kind === "array") return (v.items || []).map(__irValueFromPlanValue);
            if (v.kind === "object") {
                const out = {};
                for (const [k, child] of Object.entries(v.fields || {})) out[k] = __irValueFromPlanValue(child);
                return out;
            }
            return null;
        }

        function __validatePredicateValue(p) {
            if (p && p.value && p.value.kind === "helper") {
                throw new Error("Code Mode DSL error: predicate helper values such as " + String(p.value.name || "helper") + "(...) are not executable in Code Mode filters; pass a literal/entity ref key or precompute the value.");
            }
        }

        function __hasIrHole(v) {
            if (!v || typeof v !== "object") return false;
            if (v.__plasm_hole) return true;
            if (Array.isArray(v)) return v.some(__hasIrHole);
            return Object.values(v).some(__hasIrHole);
        }

        function __irPredicateFromPlan(p) {
            __validatePredicateValue(p);
            const op = { eq: "=", ne: "!=", lt: "<", lte: "<=", gt: ">", gte: ">=", contains: "contains", in: "in", exists: "exists" }[p.op] || "=";
            return { type: "comparison", field: (p.field_path || []).join("."), op, value: __irValueFromPlanValue(p.value) };
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

        function __normalizeAggregates(aggregates) {
            const specs = aggregates || [];
            if (!Array.isArray(specs) || specs.length === 0) {
                throw new Error("Code Mode DSL error: Plan.aggregate/groupBy.aggregate requires at least one aggregate spec");
            }
            for (const spec of specs) {
                const fn = String(spec && spec.function || "");
                if (fn !== "count") {
                    if (!Array.isArray(spec && spec.field) || spec.field.length === 0) {
                        throw new Error("Code Mode DSL error: aggregate `" + String(spec && spec.name || fn || "<unnamed>") + "` with function `" + fn + "` requires a non-empty field path");
                    }
                }
            }
            return specs;
        }

        function __planIr(expr, projection, displayExpr) {
            return { expr, projection: projection && projection.length ? projection : undefined, display_expr: displayExpr };
        }

        const __plasmRelations = {};
        const __plasmEntityKeyVars = {};

        function __relationsFor(entry_id, entity) {
            return (((__plasmRelations || {})[entry_id] || {})[entity] || []);
        }

        function __keyVarsFor(entry_id, entity) {
            return (((__plasmEntityKeyVars || {})[entry_id] || {})[entity] || []);
        }

        function __capabilityByNameOrKind(capabilities, kind, name) {
            const caps = capabilities || [];
            if (name) {
                const byName = caps.find(c => c.name === name);
                if (byName) return byName;
            }
            return caps.find(c => c.kind === kind) || {};
        }

        function __isWholeSymbolicRow(v) {
            if (v && v.__bindingPath) return (v.__bindingFieldPath || []).length === 0;
            if (v && v.__planValue && v.__planValue.kind === "node_symbol") return (v.__planValue.path || []).length === 0;
            return false;
        }

        function __normalizeEntityRefParam(value, param, entry_id) {
            if (value == null) return value;
            const name = String(param && param.name || "entity_ref");
            const target = param && param.entity_ref_target ? String(param.entity_ref_target) : "";
            if (__isEntityRefValue(value)) {
                if (target && value.entity && String(value.entity) !== target) {
                    throw new Error("Code Mode DSL error: entity_ref input '" + name + "' expects " + target + " but got " + value.entity);
                }
                return value.key;
            }
            if (value && value.__plasmRef) {
                const ref = value.__plasmRef;
                if (target && ref.entity && String(ref.entity) !== target) {
                    throw new Error("Code Mode DSL error: entity_ref input '" + name + "' expects " + target + " but got " + ref.entity);
                }
                return ref.key;
            }
            if (__isWholeSymbolicRow(value) || (value && value.__planNodeId)) {
                throw new Error("Code Mode DSL error: entity_ref input '" + name + "' expects a scalar reference key, not a whole row/read handle; pass ." + "id or entityRef(...)");
            }
            const keyVars = target ? __keyVarsFor(entry_id, target) : [];
            if (value && typeof value === "object" && !Array.isArray(value) && !__isSpecial(value)) {
                if ((keyVars || []).length > 1) return __entityKeyValue(target || name, keyVars, value);
                throw new Error("Code Mode DSL error: entity_ref input '" + name + "' expects a scalar reference key, not an object");
            }
            if (value && typeof value === "object" && (__isPlanSource(value) || __isPlanEffect(value) || __hasBrand(value, __BRAND_BUILDER) || value.__toPlanHandle)) {
                throw new Error("Code Mode DSL error: entity_ref input '" + name + "' expects a scalar reference key or get(...) ref handle, not a query/search/list handle");
            }
            return value;
        }

        function __normalizeEntityRefInputs(input, cap, entry_id) {
            if (!input || typeof input !== "object" || Array.isArray(input) || __isSpecial(input)) return input;
            const params = (cap && cap.input_parameters) || [];
            if (!params.some(p => p && p.type === "entity_ref")) return input;
            const out = Object.assign({}, input);
            for (const param of params) {
                if (!param || param.type !== "entity_ref") continue;
                const name = String(param.name || "");
                if (!name || !Object.prototype.hasOwnProperty.call(out, name)) continue;
                out[name] = __normalizeEntityRefParam(out[name], param, entry_id);
            }
            return out;
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
            const cap = __capability(capabilities || [], kind);
            if (kind === "search") {
                const parsed = __searchInput(input, searchParam);
                searchText = parsed.text;
                baseFilters = parsed.filters || {};
            } else {
                baseFilters = input || {};
            }
            baseFilters = __normalizeEntityRefInputs(baseFilters, cap, entry_id);
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
                    const irFilters = Object.assign({}, baseFilters || {});
                    if (kind === "search") irFilters[searchParam || "q"] = searchText;
                    const predicate = __irPredicates(irFilters, extraPredicates);
                    const input_bindings = __bindingsFromInput(irFilters);
                    const uses_result = __usesFromBindings(input_bindings);
                    const irExpr = {
                        op: "query",
                        entity,
                        predicate: predicate || undefined,
                        projection: projection.length ? projection : undefined,
                        capability_name: cap.name || undefined,
                    };
                    const ir = __planIr(irExpr, projection, expr);
                    const templated = __hasIrHole(irExpr);
                    const node = {
                        kind,
                        qualified_entity: { entry_id, entity },
                        expr,
                        ir: templated ? undefined : ir,
                        ir_template: templated ? Object.assign({ input_bindings }, ir) : undefined,
                        input_bindings,
                        effect_class: "read",
                        result_shape: "list",
                        projection,
                        predicates,
                        depends_on: uses_result.map(u => u.node),
                        uses_result,
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
            let projection = [];
            const targetRelations = __relationsFor(relation.entry_id, relation.target);
            const builder = {
                __toPlanHandle(id) {
                    return this.as(id);
                },
                select(...fields) {
                    projection = __projectionFields(fields);
                    return this;
                },
                as(id) {
                    const sourcePlan = __sourcePlan(source, id);
                    const sourceId = sourcePlan.sourceId;
                    const nodes = sourcePlan.nodes.slice();
                    const sourceNode = nodes[nodes.length - 1];
                    const sourceIr = sourceNode && (sourceNode.ir || (sourceNode.relation && sourceNode.relation.ir));
                    const sourceExpr = sourceNode && (sourceNode.expr || (sourceNode.relation && sourceNode.relation.expr));
                    if (!sourceNode || !sourceIr) {
                        throw new Error("Relation traversal requires a source node with Plasm IR");
                    }
                    const expr = __replaceProjection(sourceExpr + "." + relation.name, projection);
                    const irExpr = { op: "chain", source: sourceIr.expr, selector: relation.name, step: { type: "auto_get" }, projection: projection.length ? projection : undefined };
                    const ir = __planIr(irExpr, projection, expr);
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
                        projection,
                        predicates: [],
                        depends_on: [sourceId],
                        uses_result: [{ node: sourceId, as: "source" }],
                    };
                    node.__relations = targetRelations;
                    nodes.push(node);
                    const handle = __nodeHandle(id, node);
                    handle.__planNodes = nodes;
                    return handle;
                }
            };
            return __attachPlanSourceMethods(__planBuilder(builder, true), targetRelations);
        }

        export function makeEntity(entry_id, entity, relations, searchParam, capabilities, keyVars) {
            return {
                query(filters) {
                    return __surfaceBuilder(entry_id, entity, "query", filters, relations || [], null, capabilities || []);
                },
                search(input) {
                    return __surfaceBuilder(entry_id, entity, "search", input, relations || [], searchParam || "q", capabilities || []);
                },
                get(id) {
                    const key = __entityKeyValue(entity, keyVars || [], id);
                    const display = entity + "(" + __displayEntityKey(entity, keyVars || [], key) + ")";
                    const input_bindings = __bindingsFromInput({ id });
                    const uses_result = __usesFromBindings(input_bindings);
                    const irExpr = { op: "get", ref: { entity_type: entity, key } };
                    const ir = __planIr(irExpr, [], display);
                    const templated = __hasIrHole(irExpr);
                    const node = {
                        kind: "get",
                        qualified_entity: { entry_id, entity },
                        expr: display,
                        ir: templated ? undefined : ir,
                        ir_template: templated ? Object.assign({ input_bindings }, ir) : undefined,
                        effect_class: "read",
                        result_shape: "single",
                        projection: [],
                        predicates: [],
                        input_bindings,
                        depends_on: uses_result.map(u => u.node),
                        uses_result,
                    };
                    node.__relations = relations || [];
                    __attachRefMetadata(node, { entry_id, entity, key, key_vars: keyVars || [] });
                    return __attachPlanSourceMethods(__planEffect(node, true), relations || []);
                },
                create(input) {
                    const cap = __capability(capabilities || [], "create");
                    const normalizedInput = __normalizeEntityRefInputs(input || {}, cap, entry_id);
                    const expr = entity + ".create" + __callArgs(normalizedInput || {});
                    const input_bindings = __bindingsFromInput(normalizedInput || {});
                    const uses_result = __usesFromBindings(input_bindings);
                    const irExpr = { op: "create", capability: cap.name || (entity.toLowerCase() + "_create"), entity, input: __irValue(normalizedInput || {}) };
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
                            const cap = __capabilityByNameOrKind(capabilities || [], "action", method);
                            const normalizedInput = __normalizeEntityRefInputs(input || {}, cap, entry_id);
                            const expr_template = entity + "(" + __quote(id) + ")." + method + __callArgs(normalizedInput || {});
                            const input_bindings = __bindingsFromInput(normalizedInput || {});
                            if (id && id.__bindingPath) input_bindings.push({ from: id.__bindingPath, to: "id" });
                            const symbolicId = __symbolicStringParts(id);
                            if (symbolicId) input_bindings.push({ from: symbolicId.binding + (symbolicId.path.length ? "." + symbolicId.path.join(".") : ""), to: "id" });
                            const uses_result = __usesFromBindings(input_bindings);
                            const targetKey = __irValue(id);
                            const irExpr = {
                                op: "invoke",
                                capability: method,
                                target: { entity_type: entity, key: targetKey },
                                input: input == null ? undefined : __irValue(normalizedInput),
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
        let key_vars_json = serde_json::to_string(&q.key_vars).unwrap_or_else(|_| "[]".to_string());
        let _ = writeln!(
            &mut out,
            "globalThis.plasm[{alias}] = globalThis.plasm[{alias}] || {{}};\n\
             __plasmRelations[{entry_id}] = __plasmRelations[{entry_id}] || {{}};\n\
             __plasmRelations[{entry_id}][{entity}] = {relations_json};\n\
             __plasmEntityKeyVars[{entry_id}] = __plasmEntityKeyVars[{entry_id}] || {{}};\n\
             __plasmEntityKeyVars[{entry_id}][{entity}] = {key_vars_json};\n\
             globalThis.plasm[{alias}][{entity}] = makeEntity({entry_id}, {entity}, {relations_json}, {search_param}, {capabilities_json}, {key_vars_json});"
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
    fn plasm_plan_return_emits_single_and_parallel() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let single: String = ctx.eval(
                "const Product = makeEntity('acme', 'Product'); \
                 const detail = __plasmBind(Product.get('p1'), 'detail'); \
                 Plan.return(detail)",
            )?;
            let v: serde_json::Value = serde_json::from_str(&single).expect("json");
            assert_eq!(v["return"]["kind"], "node");
            assert_eq!(v["return"]["node"], "detail");
            assert_eq!(v["nodes"][0]["id"], "detail");

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
            assert!(msg.contains("object maps are not supported"), "{msg}");
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
    fn relation_read_source_supports_select_and_limit() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _v: () = ctx.eval(flat.as_str())?;
            let s: String = ctx
                .eval("const rels = [{ name: 'category', target: 'Category', cardinality: 'one', entry_id: 'acme' }]; const Product = makeEntity('acme', 'Product', rels); const limited = __plasmBind(Plan.limit(Product.get('p1').category().select('name'), 10), 'limited'); Plan.return(limited)")
                .expect("plan");
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert_eq!(v["nodes"][1]["kind"], "relation");
            assert_eq!(v["nodes"][1]["relation"]["expr"], "Product(\"p1\").category[name]");
            assert_eq!(v["nodes"][1]["relation"]["ir"]["projection"][0], "name");
            assert_eq!(v["nodes"][2]["compute"]["op"]["kind"], "limit");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn relation_read_source_exposes_target_relations() -> QjResult<()> {
        let delta = FacadeDeltaV1 {
            version: 1,
            catalog_entry_ids: vec!["acme".to_string()],
            catalog_aliases: vec![],
            qualified_entities: vec![
                crate::delta::QualifiedEntitySurface {
                    entry_id: "acme".to_string(),
                    catalog_alias: "acme".to_string(),
                    entity: "Product".to_string(),
                    description: None,
                    e_index: Some(1),
                    key_vars: vec![],
                    fields: vec![],
                    relations: vec![crate::delta::FacadeRelation {
                        name: "category".to_string(),
                        description: None,
                        target: "Category".to_string(),
                        cardinality: "one".to_string(),
                        materialize: None,
                    }],
                    capabilities: vec![],
                },
                crate::delta::QualifiedEntitySurface {
                    entry_id: "acme".to_string(),
                    catalog_alias: "acme".to_string(),
                    entity: "Category".to_string(),
                    description: None,
                    e_index: Some(2),
                    key_vars: vec![],
                    fields: vec![],
                    relations: vec![crate::delta::FacadeRelation {
                        name: "department".to_string(),
                        description: None,
                        target: "Department".to_string(),
                        cardinality: "one".to_string(),
                        materialize: None,
                    }],
                    capabilities: vec![],
                },
                crate::delta::QualifiedEntitySurface {
                    entry_id: "acme".to_string(),
                    catalog_alias: "acme".to_string(),
                    entity: "Department".to_string(),
                    description: None,
                    e_index: Some(3),
                    key_vars: vec![],
                    fields: vec![],
                    relations: vec![],
                    capabilities: vec![],
                },
            ],
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
                "try { const d = __plasmBind(plasm.acme.Product.get('p1').category().department().select('name'), 'd'); Plan.return(d) } catch (e) { JSON.stringify({ error: String((e && e.message) || e) }) }",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert!(v.get("error").is_none(), "error: {v}");
            assert_eq!(v["nodes"][2]["kind"], "relation");
            assert_eq!(
                v["nodes"][2]["relation"]["expr"],
                "Product(\"p1\").category.department[name]"
            );
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
                key_vars: vec![],
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
    fn plan_map_source_callback_becomes_staged_ir_fanout() -> QjResult<()> {
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
            assert_eq!(v["nodes"][1]["kind"], "for_each");
            assert_eq!(v["nodes"][1]["effect_class"], "read");
            assert_eq!(v["nodes"][1]["effect_template"]["kind"], "get");
            assert_eq!(
                v["nodes"][1]["effect_template"]["ir_template"]["expr"]["ref"]["key"]
                    ["__plasm_hole"]["binding"],
                "item"
            );
            assert_eq!(
                v["nodes"][1]["effect_template"]["ir_template"]["expr"]["ref"]["key"]
                    ["__plasm_hole"]["path"][0],
                "id"
            );
            assert!(!v.to_string().contains("[object Object]"), "{v}");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn compound_key_get_lowers_to_compound_ref() -> QjResult<()> {
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
                   const Repository = makeEntity('github', 'Repository', [], null, [{ kind: 'get', name: 'repo_get' }], ['owner', 'repo']); \
                   const repo = __plasmBind(Repository.get({ owner: 'ryan-s-roberts', repo: 'plasm-core' }), 'repo'); \
                   out = JSON.parse(Plan.return(repo)); \
                 } catch (e) { out = { fatal: String(e && e.message || e) }; } \
                 JSON.stringify(out)",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert!(v.get("fatal").is_none(), "{v}");
            assert_eq!(v["nodes"][0]["ir"]["expr"]["ref"]["key"]["owner"], "ryan-s-roberts");
            assert_eq!(v["nodes"][0]["ir"]["expr"]["ref"]["key"]["repo"], "plasm-core");
            assert!(v["nodes"][0]["expr"]
                .as_str()
                .is_some_and(|expr| expr.contains("owner=\"ryan-s-roberts\"") && expr.contains("repo=\"plasm-core\"")), "{v}");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn compound_key_get_accepts_plasm_string_shorthand() -> QjResult<()> {
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
                   const Repository = makeEntity('github', 'Repository', [], null, [{ kind: 'get', name: 'repo_get' }], ['owner', 'repo']); \
                   const repo = __plasmBind(Repository.get('ryan-s-roberts/plasm-core'), 'repo'); \
                   out = JSON.parse(Plan.return(repo)); \
                 } catch (e) { out = { fatal: String(e && e.message || e) }; } \
                 JSON.stringify(out)",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert!(v.get("fatal").is_none(), "{v}");
            assert_eq!(
                v["nodes"][0]["ir"]["expr"]["ref"]["key"]["owner"],
                "ryan-s-roberts"
            );
            assert_eq!(v["nodes"][0]["ir"]["expr"]["ref"]["key"]["repo"], "plasm-core");
            assert!(
                v["nodes"][0]["expr"]
                    .as_str()
                    .is_some_and(|expr| expr.contains("owner=\"ryan-s-roberts\"")
                        && expr.contains("repo=\"plasm-core\"")),
                "{v}"
            );
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn compound_key_get_rejects_three_part_string_shorthand() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let msg: String = ctx.eval(
                "let msg = ''; \
                 try { \
                   const Issue = makeEntity('github', 'Issue', [], null, [{ kind: 'get', name: 'issue_get' }], ['owner', 'repo', 'number']); \
                   Plan.return(Issue.get('ryan-s-roberts/plasm-core/42')); \
                 } catch (e) { msg = String(e && e.message || e); } \
                 msg",
            )?;
            assert!(msg.contains("string shorthand is only supported for two-part compound keys"), "{msg}");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn field_exists_and_predicate_helper_rejection_match_contract() -> QjResult<()> {
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
                   const Product = makeEntity('acme', 'Product'); \
                   const ok = __plasmBind(Product.query({}).where(field('name').exists()), 'ok'); \
                   let helperMsg = ''; \
                   try { Product.query({}).where(field('updated_at').gt(daysAgo(30))).as('bad'); } catch (e) { helperMsg = String(e && e.message || e); } \
                   out = { plan: JSON.parse(Plan.return(ok)), helperMsg }; \
                 } catch (e) { out = { fatal: String(e && e.message || e) }; } \
                 JSON.stringify(out)",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert!(v.get("fatal").is_none(), "{v}");
            assert_eq!(v["plan"]["nodes"][0]["predicates"][0]["op"], "exists");
            assert!(v["helperMsg"]
                .as_str()
                .is_some_and(|msg| msg.contains("predicate helper values")), "{v}");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn aggregate_specs_are_guarded_before_plan_json() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let msg: String = ctx.eval(
                "let msgs = []; \
                 const Product = makeEntity('acme', 'Product'); \
                 try { Plan.aggregate(Product.query({}), []); } catch (e) { msgs.push(String(e && e.message || e)); } \
                 try { Plan.aggregate(Product.query({}), [{ name: 'total', function: 'sum' }]); } catch (e) { msgs.push(String(e && e.message || e)); } \
                 JSON.stringify(msgs)",
            )?;
            let v: serde_json::Value = serde_json::from_str(&msg).expect("json");
            assert!(v[0].as_str().is_some_and(|m| m.contains("requires at least one aggregate spec")), "{v}");
            assert!(v[1].as_str().is_some_and(|m| m.contains("requires a non-empty field path")), "{v}");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn compound_key_get_rejects_incomplete_key_before_plan_json() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let msg: String = ctx.eval(
                "let msg = ''; \
                 try { \
                   const Repository = makeEntity('github', 'Repository', [], null, [{ kind: 'get', name: 'repo_get' }], ['owner', 'repo']); \
                   Plan.return(Repository.get({ repo: 'plasm-core' })); \
                 } catch (e) { msg = String(e && e.message || e); } \
                 msg",
            )?;
            assert!(msg.contains("requires compound key fields [owner, repo]"), "{msg}");
            assert!(!msg.contains("invalid type: map, expected a string"), "{msg}");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn compound_key_get_rejects_map_key_part_before_plan_json() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let msg: String = ctx.eval(
                "let msg = ''; \
                 try { \
                   const Repository = makeEntity('github', 'Repository', [], null, [{ kind: 'get', name: 'repo_get' }], ['owner', 'repo']); \
                   Plan.return(Repository.get({ owner: { login: 'ryan-s-roberts' }, repo: 'plasm-core' })); \
                 } catch (e) { msg = String(e && e.message || e); } \
                 msg",
            )?;
            assert!(msg.contains("compound key field 'owner' must lower to a string"), "{msg}");
            assert!(!msg.contains("invalid type: map, expected a string"), "{msg}");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn compound_key_get_rejects_null_before_display() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let msg: String = ctx.eval(
                "let msg = ''; \
                 try { \
                   const Repository = makeEntity('github', 'Repository', [], null, [{ kind: 'get', name: 'repo_get' }], ['owner', 'repo']); \
                   Plan.return(Repository.get(null)); \
                 } catch (e) { msg = String(e && e.message || e); } \
                 msg",
            )?;
            assert!(msg.contains("has compound key_vars [owner, repo]"), "{msg}");
            assert!(!msg.contains("Cannot read"), "{msg}");
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
                   const issue = __plasmBind(Issue.create({ team: entityRef('linear', 'Team', template`${firstTeam}`.id) }), 'issue'); \
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
            assert_eq!(
                create_node["ir_template"]["expr"]["input"]["team"]["__plasm_hole"]["node"],
                "firstTeam"
            );
            assert!(
                create_node["ir_template"]["expr"]["input"]["team"]
                    .get("key")
                    .is_none(),
                "{create_node}"
            );
            assert_eq!(create_node["depends_on"][0], "firstTeam");
            assert_eq!(create_node["uses_result"][0]["node"], "firstTeam");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn linear_team_to_issue_create_chaining_normalizes_team_ref() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let s: String = ctx.eval(
                "const Team = makeEntity('linear', 'Team'); \
                 const Issue = makeEntity('linear', 'Issue', [], null, [{ kind: 'create', name: 'issue_create' }]); \
                 const firstTeam = __plasmBind(Plan.singleton(Plan.limit(Team.query({}), 1)), 'firstTeam'); \
                 const report = __plasmBind(Issue.create({ team: entityRef('linear', 'Team', template`${firstTeam}`.id), title: 'Plasm report' }), 'report'); \
                 Plan.return(report)",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            let create_node = v["nodes"]
                .as_array()
                .and_then(|nodes| {
                    nodes
                        .iter()
                        .find(|node| node["kind"].as_str() == Some("create"))
                })
                .unwrap_or_else(|| panic!("create node missing: {v}"));
            assert_eq!(create_node["depends_on"][0], "firstTeam");
            assert_eq!(create_node["uses_result"][0]["node"], "firstTeam");
            assert_eq!(
                create_node["ir_template"]["expr"]["input"]["team"]["__plasm_hole"]["node"],
                "firstTeam"
            );
            assert!(
                create_node["ir_template"]["expr"]["input"]["team"]
                    .get("key")
                    .is_none(),
                "{create_node}"
            );
            assert!(!create_node.to_string().contains("[object Object]"), "{create_node}");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn entity_ref_create_input_accepts_symbolic_scalar_id() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let s: String = ctx.eval(
                "const Team = makeEntity('linear', 'Team'); \
                 const Issue = makeEntity('linear', 'Issue', [], null, [{ kind: 'create', name: 'issue_create', input_parameters: [{ name: 'team', type: 'entity_ref', entity_ref_target: 'Team', required: true }] }]); \
                 const firstTeam = __plasmBind(Plan.singleton(Plan.limit(Team.query({}), 1)), 'firstTeam'); \
                 __plasmSetAstHints({ node_ids: ['firstTeam'] }); \
                 const report = __plasmBind(Issue.create({ team: firstTeam.id, title: 'Plasm report' }), 'report'); \
                 Plan.return(report)",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            let create_node = v["nodes"]
                .as_array()
                .and_then(|nodes| {
                    nodes
                        .iter()
                        .find(|node| node["kind"].as_str() == Some("create"))
                })
                .unwrap_or_else(|| panic!("create node missing: {v}"));
            assert_eq!(
                create_node["ir_template"]["expr"]["input"]["team"]["__plasm_hole"]["path"][0],
                "id"
            );
            assert!(!create_node.to_string().contains("[object Object]"), "{create_node}");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn entity_ref_create_input_accepts_get_handle_with_symbolic_key() -> QjResult<()> {
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
                   const Team = makeEntity('linear', 'Team', [], null, [{ kind: 'query', name: 'team_query' }, { kind: 'get', name: 'team_get' }]); \
                   const Issue = makeEntity('linear', 'Issue', [], null, [{ kind: 'create', name: 'issue_create', input_parameters: [{ name: 'team', type: 'entity_ref', entity_ref_target: 'Team', required: true }] }]); \
                   const teams = __plasmBind(Plan.limit(Team.query({}).select('id'), 1), 'teams'); \
                   const issueFx = __plasmBind(forEach(teams, (trow) => Issue.create({ team: Team.get(trow.id), title: 'Plasm report' })), 'issueFx'); \
                   out = JSON.parse(Plan.return(issueFx)); \
                 } catch (e) { out = { fatal: String(e && e.message || e) }; } \
                 JSON.stringify(out)",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert!(v.get("fatal").is_none(), "{v}");
            let for_each = v["nodes"]
                .as_array()
                .and_then(|nodes| {
                    nodes
                        .iter()
                        .find(|node| node["kind"].as_str() == Some("for_each"))
                })
                .unwrap_or_else(|| panic!("for_each node missing: {v}"));
            assert_eq!(
                for_each["effect_template"]["ir_template"]["expr"]["input"]["team"]["__plasm_hole"]
                    ["binding"],
                "item"
            );
            assert_eq!(
                for_each["effect_template"]["ir_template"]["expr"]["input"]["team"]["__plasm_hole"]
                    ["path"][0],
                "id"
            );
            assert!(!for_each.to_string().contains("[object Object]"), "{for_each}");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn entity_ref_create_input_rejects_whole_row() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let msg: String = ctx.eval(
                "let msg = ''; \
                 try { \
                   const Team = makeEntity('linear', 'Team'); \
                   const Issue = makeEntity('linear', 'Issue', [], null, [{ kind: 'create', name: 'issue_create', input_parameters: [{ name: 'team', type: 'entity_ref', entity_ref_target: 'Team', required: true }] }]); \
                   const firstTeam = __plasmBind(Plan.singleton(Plan.limit(Team.query({}), 1)), 'firstTeam'); \
                   __plasmSetAstHints({ node_ids: ['firstTeam'] }); \
                   Plan.return(Issue.create({ team: firstTeam, title: 'Plasm report' })); \
                 } catch (e) { msg = String(e && e.message || e); } \
                 msg",
            )?;
            assert!(
                msg.contains("expects a scalar reference key, not a whole row/read handle"),
                "{msg}"
            );
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn entity_ref_create_input_rejects_wrong_runtime_entity_ref_target() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let msg: String = ctx.eval(
                "let msg = ''; \
                 try { \
                   const Issue = makeEntity('linear', 'Issue', [], null, [{ kind: 'create', name: 'issue_create', input_parameters: [{ name: 'team', type: 'entity_ref', entity_ref_target: 'Team', required: true }] }]); \
                   Plan.return(Issue.create({ team: entityRef('linear', 'Project', 'p1'), title: 'Plasm report' })); \
                 } catch (e) { msg = String(e && e.message || e); } \
                 msg",
            )?;
            assert!(
                msg.contains("entity_ref input 'team' expects Team but got Project"),
                "{msg}"
            );
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn entity_ref_values_lower_to_cml_entity_ref_payloads() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let s: String = ctx.eval(
                "const Commit = makeEntity('github', 'Commit', [], null, [{ kind: 'query', name: 'commit_query' }]); \
                 const commits = __plasmBind(Commit.query({ repository: entityRef('github', 'Repository', 'ryan-s-roberts/plasm-core') }), 'commits'); \
                 Plan.return(commits)",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert_eq!(v["nodes"][0]["kind"], "query");
            assert_eq!(v["nodes"][0]["predicates"][0]["value"]["kind"], "entity_ref_key");
            assert_eq!(
                v["nodes"][0]["predicates"][0]["value"]["key"]["value"],
                "ryan-s-roberts/plasm-core"
            );
            assert_eq!(
                v["nodes"][0]["ir"]["expr"]["predicate"]["value"],
                "ryan-s-roberts/plasm-core"
            );
            assert!(v["nodes"][0]["expr"]
                .as_str()
                .is_some_and(|expr| expr.contains("repository=\"ryan-s-roberts/plasm-core\"")), "{v}");
            assert!(
                v["nodes"][0]["ir"]["expr"]["predicate"]["value"]
                    .get("key")
                    .is_none(),
                "{v}"
            );
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn get_handle_lowers_to_entity_ref_query_input() -> QjResult<()> {
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
                   const Repository = makeEntity('github', 'Repository', [], null, [{ kind: 'get', name: 'repo_get' }], ['owner', 'repo']); \
                   const Contributor = makeEntity('github', 'Contributor', [], null, [{ kind: 'query', name: 'contributor_query', input_parameters: [{ name: 'repository', type: 'entity_ref', entity_ref_target: 'Repository', required: true }] }]); \
                   const repo = Repository.get('ryan-s-roberts/plasm-core'); \
                   const contributors = __plasmBind(Contributor.query({ repository: repo }), 'contributors'); \
                   out = JSON.parse(Plan.return(contributors)); \
                 } catch (e) { out = { fatal: String(e && e.message || e) }; } \
                 JSON.stringify(out)",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert!(v.get("fatal").is_none(), "{v}");
            assert_eq!(
                v["nodes"][0]["ir"]["expr"]["predicate"]["value"]["owner"],
                "ryan-s-roberts"
            );
            assert_eq!(
                v["nodes"][0]["ir"]["expr"]["predicate"]["value"]["repo"],
                "plasm-core"
            );
            assert!(v["nodes"][0]["expr"]
                .as_str()
                .is_some_and(|expr| expr.contains("repository={owner=\"ryan-s-roberts\", repo=\"plasm-core\"")), "{v}");
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn get_handle_object_key_lowers_to_entity_ref_query_input() -> QjResult<()> {
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
                   const Repository = makeEntity('github', 'Repository', [], null, [{ kind: 'get', name: 'repo_get' }], ['owner', 'repo']); \
                   const Contributor = makeEntity('github', 'Contributor', [], null, [{ kind: 'query', name: 'contributor_query', input_parameters: [{ name: 'repository', type: 'entity_ref', entity_ref_target: 'Repository', required: true }] }]); \
                   const repo = Repository.get({ owner: 'ryan-s-roberts', repo: 'plasm-core' }); \
                   const contributors = __plasmBind(Contributor.query({ repository: repo }), 'contributors'); \
                   out = JSON.parse(Plan.return(contributors)); \
                 } catch (e) { out = { fatal: String(e && e.message || e) }; } \
                 JSON.stringify(out)",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert!(v.get("fatal").is_none(), "{v}");
            assert_eq!(
                v["nodes"][0]["ir"]["expr"]["predicate"]["value"]["owner"],
                "ryan-s-roberts"
            );
            assert_eq!(
                v["nodes"][0]["ir"]["expr"]["predicate"]["value"]["repo"],
                "plasm-core"
            );
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn query_handle_is_not_an_entity_ref_query_input() -> QjResult<()> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;
        let js = quickjs_runtime_module_bootstrap();
        context.with(|ctx| {
            let flat = js
                .replace("export function", "function")
                .replace("export class", "class");
            let _: () = ctx.eval(flat.as_str())?;
            let msg: String = ctx.eval(
                "let msg = ''; \
                 try { \
                   const Repository = makeEntity('github', 'Repository', [], null, [{ kind: 'query', name: 'repo_query' }], ['owner', 'repo']); \
                   const Contributor = makeEntity('github', 'Contributor', [], null, [{ kind: 'query', name: 'contributor_query', input_parameters: [{ name: 'repository', type: 'entity_ref', entity_ref_target: 'Repository', required: true }] }]); \
                   Plan.return(Contributor.query({ repository: Repository.query({}) })); \
                 } catch (e) { msg = String(e && e.message || e); } \
                 msg",
            )?;
            assert!(
                msg.contains("expects a scalar reference key or get(...) ref handle"),
                "{msg}"
            );
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn entity_ref_values_lower_in_where_predicates() -> QjResult<()> {
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
                   const Commit = makeEntity('github', 'Commit', [], null, [{ kind: 'query', name: 'commit_query' }]); \
                   const repo = entityRef('github', 'Repository', 'ryan-s-roberts/plasm-core'); \
                   const commits = __plasmBind(Commit.query({}).where(field('repository').eq(repo)), 'commits'); \
                   out = JSON.parse(Plan.return(commits)); \
                 } catch (e) { out = { fatal: String(e && e.message || e) }; } \
                 JSON.stringify(out)",
            )?;
            let v: serde_json::Value = serde_json::from_str(&s).expect("json");
            assert!(v.get("fatal").is_none(), "{v}");
            assert_eq!(v["nodes"][0]["predicates"][0]["value"]["kind"], "entity_ref_key");
            assert_eq!(
                v["nodes"][0]["predicates"][0]["value"]["key"]["value"],
                "ryan-s-roberts/plasm-core"
            );
            assert_eq!(
                v["nodes"][0]["ir"]["expr"]["predicate"]["value"],
                "ryan-s-roberts/plasm-core"
            );
            assert!(!v.to_string().contains("[object Object]"), "{v}");
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
                 Plan.return([fromGet, fromSingleton])",
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
