use crate::commands::common;
use crate::PredicateAction;
use plasm_core::{type_check_predicate, Predicate, CGS};
use std::path::Path;

pub async fn execute(action: PredicateAction) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        PredicateAction::Check { schema, predicate } => {
            println!("Type-checking predicate against schema...");

            // Load schema
            if !Path::new(&schema).exists() {
                eprintln!("Error: Schema file '{}' does not exist", schema);
                return Err("Schema file not found".into());
            }

            let cgs: CGS = common::load_cgs(Path::new(&schema))
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

            // Load predicate
            let predicate_data = if Path::new(&predicate).exists() {
                std::fs::read_to_string(&predicate)?
            } else {
                predicate.clone() // Assume it's JSON directly
            };

            let pred: Predicate = serde_json::from_str(&predicate_data)?;

            // Determine which entity this predicate is for
            // For now, find the first entity that has all referenced fields
            let referenced_fields = pred.referenced_fields();
            let referenced_relations = pred.referenced_relations();

            let mut compatible_entities = Vec::new();

            for (entity_name, entity) in &cgs.entities {
                let has_all_fields = referenced_fields
                    .iter()
                    .all(|field| entity.fields.contains_key(field.as_str()));
                let has_all_relations = referenced_relations
                    .iter()
                    .all(|relation| entity.relations.contains_key(relation.as_str()));

                if has_all_fields && has_all_relations {
                    compatible_entities.push((entity_name, entity));
                }
            }

            if compatible_entities.is_empty() {
                eprintln!("✗ No compatible entities found for this predicate");
                eprintln!("  Referenced fields: {:?}", referenced_fields);
                eprintln!("  Referenced relations: {:?}", referenced_relations);
                return Err("No compatible entities".into());
            }

            // Type-check against all compatible entities
            for (entity_name, entity) in &compatible_entities {
                match type_check_predicate(&pred, entity, &[], &cgs) {
                    Ok(()) => {
                        println!("✓ Predicate is valid for entity '{}'", entity_name);
                    }
                    Err(e) => {
                        eprintln!("✗ Type error for entity '{}': {}", entity_name, e);
                        return Err(e.into());
                    }
                }
            }

            Ok(())
        }
    }
}
