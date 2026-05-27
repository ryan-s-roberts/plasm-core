//! Optional live checks against Linear GraphQL (network + `LINEAR_API_TOKEN`).
//! `cargo test -p plasm-e2e --test linear_live -- --ignored`

#[tokio::test]
#[ignore = "network + LINEAR_API_TOKEN: live Linear GraphQL"]
async fn linear_issue_search_first_page() {
    let key = std::env::var("LINEAR_API_TOKEN").expect("set LINEAR_API_TOKEN for ignored test");
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "query": r#"query($first: Int, $filter: IssueFilter) { issues(first: $first, filter: $filter) { nodes { identifier title } pageInfo { hasNextPage } } }"#,
        "variables": {"first": 2, "filter": null}
    });
    let res = client
        .post("https://api.linear.app/graphql")
        .header("Authorization", key)
        .json(&body)
        .send()
        .await
        .expect("request");
    assert!(res.status().is_success());
    let v: serde_json::Value = res.json().await.expect("json");
    if let Some(err) = v.get("errors") {
        panic!("Linear GraphQL errors: {err}");
    }
    let nodes = v["data"]["issues"]["nodes"].as_array().expect("nodes");
    assert!(nodes.len() <= 2);
}

#[tokio::test]
#[ignore = "network + LINEAR_API_TOKEN: live Linear GraphQL"]
async fn linear_viewer_query() {
    let key = std::env::var("LINEAR_API_TOKEN").expect("set LINEAR_API_TOKEN for ignored test");
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "query": r#"{ viewer { id displayName } }"#
    });
    let res = client
        .post("https://api.linear.app/graphql")
        .header("Authorization", key)
        .json(&body)
        .send()
        .await
        .expect("request");
    assert!(res.status().is_success());
    let v: serde_json::Value = res.json().await.expect("json");
    if let Some(err) = v.get("errors") {
        panic!("Linear GraphQL errors: {err}");
    }
    assert!(v["data"]["viewer"]["id"].is_string());
}
