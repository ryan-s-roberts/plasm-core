// Additive correction: produce ONLY the missing entities and shared types.
// The caller will merge these into the existing model programmatically.

function ExtractMissingEntities(
    api_summary: string,
    missing_entity_names: string,
    known_refs: string,
    known_enums: string
) -> DomainModel {
    client DeepExtract
    prompt #"
        {{ _.role("system") }}
        You are an API domain model extractor performing a targeted
        correction pass. A previous extraction missed some entities
        and shared types. You must produce ONLY the missing pieces.

        {{ _.role("user") }}
        A previous domain extraction of this API missed the following
        entities and shared types. Produce definitions for ONLY these
        missing items.

        Missing entities to define: {{ missing_entity_names }}

        Known $ref relationships from the spec:
        {{ known_refs }}

        Known enum fields from the spec:
        {{ known_enums }}

        For reference, here is the full API specification:
        ---
        {{ api_summary }}
        ---

        {{ ctx.output_format }}

        IMPORTANT: Only output the MISSING entities and shared types.
        Do not reproduce entities that were already successfully extracted.
        Each missing entity must have: name, id_field, id_type, fields.
        Relations and capabilities are optional for these supplementary entities.
    "#
}

// Shared type correction: produce only the missing shared types
// and update field references. The caller merges additively.
function ExtractMissingSharedTypes(
    api_summary: string,
    existing_entities: string,
    missing_type_issues: string
) -> DomainModel {
    client DeepExtract
    prompt #"
        {{ _.role("system") }}
        You are an API domain analyst. A previous extraction defined entities
        correctly but is missing shared enum types. You must produce ONLY
        the missing shared types with their values and usage references.

        Shared types capture domain concepts that appear in multiple places.
        For example, if an entity field and a query parameter both use
        the same enum values (e.g. ["available", "pending", "sold"]), that is ONE shared type,
        not two independent enums.

        {{ _.role("user") }}
        Here are the entities that were already extracted:
        {{ existing_entities }}

        Here are the issues to fix:
        {{ missing_type_issues }}

        For reference, the API spec:
        ---
        {{ api_summary }}
        ---

        {{ ctx.output_format }}

        IMPORTANT: Return an empty entities list (they are already correct).
        Only populate the shared_types list with the missing types.
        Each shared type needs: name, values, and used_by references.
    "#
}

// Judge function: validates semantic quality of the extraction
function ValidateDomainModel(
    api_summary: string,
    domain_model: string
) -> DomainModelValidation {
    client Fast
    prompt #"
        {{ _.role("system") }}
        You are a domain model quality reviewer. Rate the extraction
        and identify semantic issues.

        {{ _.role("user") }}
        API specification:
        ---
        {{ api_summary }}
        ---

        Extracted domain model:
        ---
        {{ domain_model }}
        ---

        {{ ctx.output_format }}
    "#
}

class DomainModelValidation {
    overall_quality string @description("GOOD, ACCEPTABLE, or NEEDS_WORK")
    issues ValidationIssue[]
    suggestions string[]
}

class ValidationIssue {
    severity string @description("ERROR, WARNING, or INFO")
    entity string? @description("Which entity this issue relates to")
    description string
}
