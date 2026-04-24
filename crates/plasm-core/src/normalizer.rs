use crate::{NormalizationError, Predicate};

const MAX_DEPTH: usize = 50; // Prevent stack overflow on deeply nested predicates

/// Normalize a predicate to canonical form.
///
/// This performs:
/// - Flattening of nested And/Or
/// - DeMorgan's law for Not
/// - Elimination of trivial True/False
/// - Deduplication of identical predicates
pub fn normalize(predicate: Predicate) -> Result<Predicate, NormalizationError> {
    if predicate.depth() > MAX_DEPTH {
        return Err(NormalizationError::MaxDepthExceeded);
    }

    let normalized = normalize_recursive(predicate);
    Ok(normalized)
}

fn normalize_recursive(predicate: Predicate) -> Predicate {
    match predicate {
        // Base cases
        Predicate::True | Predicate::False | Predicate::Comparison { .. } => predicate,

        // Normalize logical operations
        Predicate::And { args } => normalize_junction(Junction::And, args),
        Predicate::Or { args } => normalize_junction(Junction::Or, args),
        Predicate::Not { predicate } => normalize_not(*predicate),

        // Normalize relation predicates
        Predicate::ExistsRelation {
            relation,
            predicate,
        } => {
            let normalized_predicate = predicate.map(|p| Box::new(normalize_recursive(*p)));
            Predicate::ExistsRelation {
                relation,
                predicate: normalized_predicate,
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Junction {
    And,
    Or,
}

fn normalize_junction(kind: Junction, args: Vec<Predicate>) -> Predicate {
    let mut normalized_args = Vec::new();

    for arg in args {
        let normalized = normalize_recursive(arg);

        match (kind, normalized) {
            (Junction::And, Predicate::And { args: nested_args }) => {
                normalized_args.extend(nested_args);
            }
            (Junction::Or, Predicate::Or { args: nested_args }) => {
                normalized_args.extend(nested_args);
            }
            (Junction::And, Predicate::False) => return Predicate::False,
            (Junction::Or, Predicate::True) => return Predicate::True,
            (Junction::And, Predicate::True) | (Junction::Or, Predicate::False) => {}
            (_, other) => normalized_args.push(other),
        }
    }

    normalized_args.sort_by(|a, b| {
        serde_json::to_string(a)
            .unwrap_or_default()
            .cmp(&serde_json::to_string(b).unwrap_or_default())
    });
    normalized_args.dedup();

    match kind {
        Junction::And => match normalized_args.len() {
            0 => Predicate::True,
            1 => normalized_args.into_iter().next().unwrap(),
            _ => Predicate::And {
                args: normalized_args,
            },
        },
        Junction::Or => match normalized_args.len() {
            0 => Predicate::False,
            1 => normalized_args.into_iter().next().unwrap(),
            _ => Predicate::Or {
                args: normalized_args,
            },
        },
    }
}

fn normalize_not(predicate: Predicate) -> Predicate {
    let normalized = normalize_recursive(predicate);

    match normalized {
        // Double negation
        Predicate::Not { predicate } => normalize_recursive(*predicate),

        // DeMorgan's laws
        Predicate::And { args } => {
            let negated_args = args.into_iter().map(normalize_not).collect();
            normalize_junction(Junction::Or, negated_args)
        }
        Predicate::Or { args } => {
            let negated_args = args.into_iter().map(normalize_not).collect();
            normalize_junction(Junction::And, negated_args)
        }

        // Trivial cases
        Predicate::True => Predicate::False,
        Predicate::False => Predicate::True,

        // Base cases
        _ => Predicate::Not {
            predicate: Box::new(normalized),
        },
    }
}

/// Check if a predicate is in normalized form.
pub fn is_normalized(predicate: &Predicate) -> bool {
    match predicate {
        Predicate::True | Predicate::False | Predicate::Comparison { .. } => true,

        Predicate::And { args } => {
            // Check for no nested Ands, no True/False, and sorted/deduped
            let mut has_nested_and = false;
            let mut has_trivial = false;

            for arg in args {
                if !is_normalized(arg) {
                    return false;
                }
                if matches!(arg, Predicate::And { .. }) {
                    has_nested_and = true;
                }
                if arg.is_trivial() {
                    has_trivial = true;
                }
            }

            !has_nested_and && !has_trivial && is_sorted_and_deduped(args)
        }

        Predicate::Or { args } => {
            // Check for no nested Ors, no True/False, and sorted/deduped
            let mut has_nested_or = false;
            let mut has_trivial = false;

            for arg in args {
                if !is_normalized(arg) {
                    return false;
                }
                if matches!(arg, Predicate::Or { .. }) {
                    has_nested_or = true;
                }
                if arg.is_trivial() {
                    has_trivial = true;
                }
            }

            !has_nested_or && !has_trivial && is_sorted_and_deduped(args)
        }

        Predicate::Not { predicate } => {
            match predicate.as_ref() {
                // No double negation
                Predicate::Not { .. } => false,
                // No negated And/Or (should be DeMorgan'd)
                Predicate::And { .. } | Predicate::Or { .. } => false,
                // No negated trivials
                Predicate::True | Predicate::False => false,
                // Other cases are ok if normalized
                _ => is_normalized(predicate),
            }
        }

        Predicate::ExistsRelation { predicate, .. } => {
            predicate.as_ref().is_none_or(|p| is_normalized(p))
        }
    }
}

fn is_sorted_and_deduped(args: &[Predicate]) -> bool {
    let serialized: Vec<String> = args
        .iter()
        .map(|p| serde_json::to_string(p).unwrap_or_default())
        .collect();
    let mut sorted = serialized.clone();
    sorted.sort();
    sorted.dedup();
    serialized == sorted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CompOp;

    #[test]
    fn test_flatten_and() {
        let predicate = Predicate::And {
            args: vec![
                Predicate::comparison("a", CompOp::Eq, "1"),
                Predicate::And {
                    args: vec![
                        Predicate::comparison("b", CompOp::Eq, "2"),
                        Predicate::comparison("c", CompOp::Eq, "3"),
                    ],
                },
            ],
        };

        let normalized = normalize(predicate).unwrap();

        if let Predicate::And { args } = normalized {
            assert_eq!(args.len(), 3);
            // All three comparisons should be at the same level
        } else {
            panic!("Expected And predicate");
        }
    }

    #[test]
    fn test_eliminate_true_in_and() {
        let predicate = Predicate::And {
            args: vec![
                Predicate::comparison("a", CompOp::Eq, "1"),
                Predicate::True,
                Predicate::comparison("b", CompOp::Eq, "2"),
            ],
        };

        let normalized = normalize(predicate).unwrap();

        if let Predicate::And { args } = normalized {
            assert_eq!(args.len(), 2);
            // True should be eliminated
        } else {
            panic!("Expected And predicate");
        }
    }

    #[test]
    fn test_false_in_and_makes_false() {
        let predicate = Predicate::And {
            args: vec![
                Predicate::comparison("a", CompOp::Eq, "1"),
                Predicate::False,
                Predicate::comparison("b", CompOp::Eq, "2"),
            ],
        };

        let normalized = normalize(predicate).unwrap();

        assert_eq!(normalized, Predicate::False);
    }

    #[test]
    fn test_demorgans_law() {
        let predicate = Predicate::Not {
            predicate: Box::new(Predicate::And {
                args: vec![
                    Predicate::comparison("a", CompOp::Eq, "1"),
                    Predicate::comparison("b", CompOp::Eq, "2"),
                ],
            }),
        };

        let normalized = normalize(predicate).unwrap();

        // Should become Or(Not(a=1), Not(b=2))
        if let Predicate::Or { args } = normalized {
            assert_eq!(args.len(), 2);
            for arg in args {
                assert!(matches!(arg, Predicate::Not { .. }));
            }
        } else {
            panic!("Expected Or predicate after DeMorgan's law");
        }
    }

    #[test]
    fn test_double_negation() {
        let predicate = Predicate::Not {
            predicate: Box::new(Predicate::Not {
                predicate: Box::new(Predicate::comparison("a", CompOp::Eq, "1")),
            }),
        };

        let normalized = normalize(predicate).unwrap();

        // Should become just a=1
        assert!(matches!(normalized, Predicate::Comparison { .. }));
    }

    #[test]
    fn test_deduplication() {
        let predicate = Predicate::And {
            args: vec![
                Predicate::comparison("a", CompOp::Eq, "1"),
                Predicate::comparison("b", CompOp::Eq, "2"),
                Predicate::comparison("a", CompOp::Eq, "1"), // duplicate
            ],
        };

        let normalized = normalize(predicate).unwrap();

        if let Predicate::And { args } = normalized {
            assert_eq!(args.len(), 2); // Duplicate should be removed
        } else {
            panic!("Expected And predicate");
        }
    }

    #[test]
    fn test_single_arg_and_becomes_arg() {
        let predicate = Predicate::And {
            args: vec![Predicate::comparison("a", CompOp::Eq, "1")],
        };

        let normalized = normalize(predicate).unwrap();

        assert!(matches!(normalized, Predicate::Comparison { .. }));
    }

    #[test]
    fn test_empty_and_becomes_true() {
        let predicate = Predicate::And { args: vec![] };

        let normalized = normalize(predicate).unwrap();

        assert_eq!(normalized, Predicate::True);
    }

    #[test]
    fn test_idempotency() {
        let predicate = Predicate::And {
            args: vec![
                Predicate::comparison("a", CompOp::Eq, "1"),
                Predicate::comparison("b", CompOp::Gt, 2.0),
            ],
        };

        let normalized1 = normalize(predicate.clone()).unwrap();
        let normalized2 = normalize(normalized1.clone()).unwrap();

        assert_eq!(normalized1, normalized2);
        assert!(is_normalized(&normalized2));
    }
}
