//! Normalized matching between CGS capability names and CLI tokens (kebab vs snake).

/// Leak a String to get a `&'static str`, suitable for clap identifiers.
pub(crate) fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// Lowercase and map hyphens to underscores for comparisons.
pub(crate) fn normalize_cli_token(s: &str) -> String {
    s.to_lowercase().replace('-', "_")
}

/// Stable clap arg id for a CML path variable (use with `.long(path_param_long_flag(name))`).
pub(crate) fn path_param_arg_id(variable_name: &str) -> &'static str {
    let normalized = variable_name.replace(['-', ' '], "_");
    leak(format!("path_param__{normalized}"))
}

pub(crate) fn path_param_long_flag(variable_name: &str) -> String {
    variable_name.replace('_', "-")
}

/// Relation / capability subcommand from domain name (`team_members` → `team-members`).
pub(crate) fn relation_subcommand_kebab(domain_name: &str) -> String {
    domain_name.to_lowercase().replace('_', "-")
}

/// Naive pluralization for CLI subcommand names: `Order` → `orders`, `Status` → `statuses`.
pub(crate) fn pluralize_entity(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.ends_with('s')
        || lower.ends_with("sh")
        || lower.ends_with("ch")
        || lower.ends_with('x')
        || lower.ends_with('z')
    {
        format!("{lower}es")
    } else if lower.ends_with('y')
        && !lower.ends_with("ay")
        && !lower.ends_with("ey")
        && !lower.ends_with("oy")
        && !lower.ends_with("uy")
    {
        let stem = &lower[..lower.len() - 1];
        format!("{stem}ies")
    } else {
        format!("{lower}s")
    }
}

/// Convert a field name (snake_case or camelCase) to kebab-case for CLI subcommands.
/// `petId` → `pet-id`, `team_members` → `team-members`, `pokemon_id` → `pokemon-id`.
pub(crate) fn field_subcommand_kebab(field_name: &str) -> String {
    let mut result = String::with_capacity(field_name.len() + 4);
    for (i, ch) in field_name.chars().enumerate() {
        if ch == '_' {
            result.push('-');
        } else if ch.is_uppercase() && i > 0 {
            result.push('-');
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch.to_ascii_lowercase());
        }
    }
    result
}
