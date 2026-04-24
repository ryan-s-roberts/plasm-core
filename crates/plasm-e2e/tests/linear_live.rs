//! Optional live checks against Linear GraphQL (network + `LINEAR_API_TOKEN`).
//! `cargo test -p plasm-e2e --test linear_live -- --ignored`

#[tokio::test]
#[ignore = "network + LINEAR_API_TOKEN: live Linear GraphQL"]
async fn linear_issues_relay_first_page() {
    let key = std::env::var("LINEAR_API_TOKEN").expect("set LINEAR_API_TOKEN for ignored test");
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "query": r#"query($first: Int, $after: String) { issues(first: $first, after: $after) { nodes { id } pageInfo { hasNextPage endCursor } } }"#,
        "variables": {"first": 2, "after": null}
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
    if v.get("errors").is_some() {
        panic!("Linear GraphQL errors: {}", v);
    }
    let nodes = v["data"]["issues"]["nodes"]
        .as_array()
        .expect("data.issues.nodes");
    assert!(nodes.len() <= 2, "requested first: 2");
    let pi = &v["data"]["issues"]["pageInfo"];
    assert!(pi.get("hasNextPage").is_some());
}
