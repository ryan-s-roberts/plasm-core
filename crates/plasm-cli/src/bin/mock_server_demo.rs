use plasm_core::{FieldSchema, FieldType, ResourceSchema, CGS};
use plasm_mock::{start_server, MockResource, MockStore};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 STEP 8: Mock Server Integration Demo");
    println!("Starting mock server with test data...\n");

    // Create schema
    let mut cgs = CGS::new();
    let account = ResourceSchema {
        name: "Account".into(),
        description: String::new(),
        id_field: "id".into(),
        id_format: None,
        id_from: None,
        fields: vec![
            FieldSchema {
                name: "id".into(),
                description: String::new(),
                field_type: FieldType::String,
                value_format: None,
                allowed_values: None,
                required: true,
                array_items: None,
                string_semantics: None,
                agent_presentation: None,
                mime_type_hint: None,
                attachment_media: None,
                wire_path: None,
                derive: None,
            },
            FieldSchema {
                name: "name".into(),
                description: String::new(),
                field_type: FieldType::String,
                value_format: None,
                allowed_values: None,
                required: true,
                array_items: None,
                string_semantics: None,
                agent_presentation: None,
                mime_type_hint: None,
                attachment_media: None,
                wire_path: None,
                derive: None,
            },
            FieldSchema {
                name: "revenue".into(),
                description: String::new(),
                field_type: FieldType::Number,
                value_format: None,
                allowed_values: None,
                required: false,
                array_items: None,
                string_semantics: None,
                agent_presentation: None,
                mime_type_hint: None,
                attachment_media: None,
                wire_path: None,
                derive: None,
            },
            FieldSchema {
                name: "region".into(),
                description: String::new(),
                field_type: FieldType::Select,
                value_format: None,
                allowed_values: Some(vec![
                    "EMEA".to_string(),
                    "APAC".to_string(),
                    "AMER".to_string(),
                ]),
                required: false,
                array_items: None,
                string_semantics: None,
                agent_presentation: None,
                mime_type_hint: None,
                attachment_media: None,
                wire_path: None,
                derive: None,
            },
        ],
        relations: vec![],
        expression_aliases: vec![],
        implicit_request_identity: false,
        key_vars: vec![],
        abstract_entity: false,
        domain_projection_examples: false,
        primary_read: None,
    };
    cgs.add_resource(account)?;

    // Create mock store with test data
    let mut store = MockStore::new(cgs);

    // Add test accounts
    let acc1 = MockResource::new("acc-1")
        .with_field("name", "Acme Corp")
        .with_field("revenue", 1500.0)
        .with_field("region", "EMEA");

    let acc2 = MockResource::new("acc-2")
        .with_field("name", "Beta Inc")
        .with_field("revenue", 800.0)
        .with_field("region", "APAC");

    let acc3 = MockResource::new("acc-3")
        .with_field("name", "Gamma LLC")
        .with_field("revenue", 1200.0)
        .with_field("region", "EMEA");

    store.add_resource("Account", acc1)?;
    store.add_resource("Account", acc2)?;
    store.add_resource("Account", acc3)?;

    println!("✓ Mock server data loaded:");
    println!("  - 3 Account entities with test data");
    println!("  - Regions: EMEA (2), APAC (1)");
    println!("  - Revenue range: 800 - 1500");

    println!("\nStarting server on port 3001...");
    start_server(store, 3001).await?;

    Ok(())
}
