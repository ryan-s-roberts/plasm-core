//! Normalize `--backend` for known schemas so common mistakes still hit the right host.

/// True when `--schema` points at the bundled `apis/github` tree (directory `github` or a file inside it).
pub(crate) fn is_bundled_github_schema(schema_path: &str) -> bool {
    let path = std::path::Path::new(schema_path);
    if path.file_name().is_some_and(|n| n == "github") {
        return true;
    }
    path.parent()
        .is_some_and(|p| p.file_name().is_some_and(|n| n == "github"))
}

fn http_https_host(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))?;
    let host_port = rest.split('/').next()?.split('?').next()?;
    let host = host_port.split(':').next()?;
    Some(host)
}

/// GitHub REST API lives on `api.github.com`. Using `https://github.com` as base yields HTML (404) for `/user`, etc.
pub fn normalize_live_backend_url(schema_path: &str, backend: &str) -> String {
    if !is_bundled_github_schema(schema_path) {
        return backend.to_string();
    }
    let Some(host) = http_https_host(backend) else {
        return backend.to_string();
    };
    let host_lc = host.to_ascii_lowercase();
    if host_lc == "github.com" || host_lc == "www.github.com" {
        tracing::warn!(
            target: "plasm_agent",
            from = %backend,
            "using https://api.github.com for GitHub REST (--backend pointed at github.com, which serves the website, not the API)"
        );
        return "https://api.github.com".to_string();
    }
    backend.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_github_schema_dir_and_domain_file() {
        assert!(is_bundled_github_schema("apis/github"));
        assert!(is_bundled_github_schema("apis/github/domain.yaml"));
        assert!(!is_bundled_github_schema("fixtures/schemas/petstore"));
    }

    #[test]
    fn rewrites_github_com_to_api() {
        assert_eq!(
            normalize_live_backend_url("apis/github", "https://github.com"),
            "https://api.github.com"
        );
        assert_eq!(
            normalize_live_backend_url("apis/github", "http://www.github.com/"),
            "https://api.github.com"
        );
        assert_eq!(
            normalize_live_backend_url("fixtures/schemas/petstore", "https://github.com"),
            "https://github.com"
        );
        assert_eq!(
            normalize_live_backend_url("apis/github", "https://api.github.com"),
            "https://api.github.com"
        );
    }
}
