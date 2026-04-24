//! Optional live checks against the public GraphQLZero API (network).
//! Default `cargo test` skips these; run with `cargo test -p plasm-e2e --test graphqlzero_live -- --ignored`.

#[tokio::test]
#[ignore = "network: public GraphQLZero API"]
async fn graphqlzero_posts_page_returns_requested_limit() {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "query": r#"query($o: PageQueryOptions){ posts(options: $o) { data { id } meta { totalCount } } }"#,
        "variables": {"o": {"paginate": {"page": 1, "limit": 2}}}
    });
    let res = client
        .post("https://graphqlzero.almansi.me/api")
        .json(&body)
        .send()
        .await
        .expect("request");
    assert!(res.status().is_success());
    let v: serde_json::Value = res.json().await.expect("json");
    let arr = v["data"]["posts"]["data"]
        .as_array()
        .expect("data.posts.data array");
    assert_eq!(arr.len(), 2, "wire shape for PageQueryOptions pagination");
}
