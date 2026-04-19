//! relationship deduplication
//!
//! merges bidirectional relationships (A.field1→B and B.field2→A) into a
//! single edge, reducing visual clutter in rendered diagrams.

use crate::parse::{Cardinality, Schema};
use std::collections::BTreeMap;

type EdgeGroup = (Vec<EdgeSide>, Vec<EdgeSide>);

/// one side of a relationship edge
#[derive(Debug, Clone)]
pub struct EdgeSide {
    pub field_name: String,
    pub cardinality: Cardinality,
}

/// a possibly-merged relationship edge between two entities.
///
/// for unidirectional edges, `left` is the source and `right` is the target.
/// for bidirectional edges, `left` is the alphabetically first entity.
#[derive(Debug)]
pub struct MergedEdge {
    pub left: String,
    pub right: String,
    /// relationship from left to right
    pub left_to_right: EdgeSide,
    /// relationship from right to left (present only for bidirectional edges)
    pub right_to_left: Option<EdgeSide>,
}

/// deduplicate bidirectional relationships in a schema.
///
/// groups relationships by canonical entity pair (sorted names), merges
/// pairs that exist in both directions into a single [`MergedEdge`], and
/// leaves unidirectional and self-referential relationships untouched.
pub fn deduplicate(schema: &Schema) -> Vec<MergedEdge> {
    // key: (min_name, max_name)
    // value: (forward edges left→right, reverse edges right→left)
    let mut groups: BTreeMap<(&str, &str), EdgeGroup> = BTreeMap::new();

    let mut self_refs: Vec<MergedEdge> = Vec::new();

    for entity in &schema.entities {
        for rel in &entity.relationships {
            if entity.name == rel.target {
                self_refs.push(MergedEdge {
                    left: entity.name.clone(),
                    right: rel.target.clone(),
                    left_to_right: EdgeSide {
                        field_name: rel.field_name.clone(),
                        cardinality: rel.cardinality,
                    },
                    right_to_left: None,
                });
                continue;
            }

            let side = EdgeSide {
                field_name: rel.field_name.clone(),
                cardinality: rel.cardinality,
            };

            if entity.name <= rel.target {
                let key = (entity.name.as_str(), rel.target.as_str());
                groups.entry(key).or_default().0.push(side);
            } else {
                let key = (rel.target.as_str(), entity.name.as_str());
                groups.entry(key).or_default().1.push(side);
            }
        }
    }

    let mut edges = Vec::new();

    for ((left, right), (forward, reverse)) in &groups {
        let mut fwd_iter = forward.iter();
        let mut rev_iter = reverse.iter();

        loop {
            match (fwd_iter.next(), rev_iter.next()) {
                (Some(fwd), Some(rev)) => {
                    edges.push(MergedEdge {
                        left: left.to_string(),
                        right: right.to_string(),
                        left_to_right: fwd.clone(),
                        right_to_left: Some(rev.clone()),
                    });
                }
                (Some(fwd), None) => {
                    edges.push(MergedEdge {
                        left: left.to_string(),
                        right: right.to_string(),
                        left_to_right: fwd.clone(),
                        right_to_left: None,
                    });
                }
                (None, Some(rev)) => {
                    // reverse-only: preserve original direction (right→left)
                    edges.push(MergedEdge {
                        left: right.to_string(),
                        right: left.to_string(),
                        left_to_right: rev.clone(),
                        right_to_left: None,
                    });
                }
                (None, None) => break,
            }
        }
    }

    edges.append(&mut self_refs);

    edges.sort_by(|a, b| {
        (&a.left, &a.right, &a.left_to_right.field_name).cmp(&(
            &b.left,
            &b.right,
            &b.left_to_right.field_name,
        ))
    });

    edges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{Entity, Relationship};

    #[test]
    fn test_bidirectional_merge() {
        let schema = Schema {
            entities: vec![
                Entity {
                    name: "A".to_string(),
                    attributes: vec![],
                    relationships: vec![Relationship {
                        field_name: "bs".to_string(),
                        target: "B".to_string(),
                        cardinality: Cardinality::Many,
                    }],
                },
                Entity {
                    name: "B".to_string(),
                    attributes: vec![],
                    relationships: vec![Relationship {
                        field_name: "a".to_string(),
                        target: "A".to_string(),
                        cardinality: Cardinality::One,
                    }],
                },
            ],
        };

        let edges = deduplicate(&schema);
        assert_eq!(edges.len(), 1);

        let edge = &edges[0];
        assert_eq!(edge.left, "A");
        assert_eq!(edge.right, "B");
        assert_eq!(edge.left_to_right.field_name, "bs");
        assert_eq!(edge.left_to_right.cardinality, Cardinality::Many);

        let rev = edge.right_to_left.as_ref().unwrap();
        assert_eq!(rev.field_name, "a");
        assert_eq!(rev.cardinality, Cardinality::One);
    }

    #[test]
    fn test_unidirectional_stays() {
        let schema = Schema {
            entities: vec![
                Entity {
                    name: "A".to_string(),
                    attributes: vec![],
                    relationships: vec![Relationship {
                        field_name: "b".to_string(),
                        target: "B".to_string(),
                        cardinality: Cardinality::One,
                    }],
                },
                Entity {
                    name: "B".to_string(),
                    attributes: vec![],
                    relationships: vec![],
                },
            ],
        };

        let edges = deduplicate(&schema);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].left, "A");
        assert_eq!(edges[0].right, "B");
        assert!(edges[0].right_to_left.is_none());
    }

    #[test]
    fn test_self_referential_not_merged() {
        let schema = Schema {
            entities: vec![Entity {
                name: "Tree".to_string(),
                attributes: vec![],
                relationships: vec![
                    Relationship {
                        field_name: "parent".to_string(),
                        target: "Tree".to_string(),
                        cardinality: Cardinality::One,
                    },
                    Relationship {
                        field_name: "children".to_string(),
                        target: "Tree".to_string(),
                        cardinality: Cardinality::Many,
                    },
                ],
            }],
        };

        let edges = deduplicate(&schema);
        assert_eq!(edges.len(), 2);
        assert!(edges.iter().all(|e| e.right_to_left.is_none()));
        assert!(edges.iter().all(|e| e.left == "Tree" && e.right == "Tree"));
    }

    #[test]
    fn test_reverse_only_preserves_direction() {
        // B→A exists but A→B does not
        let schema = Schema {
            entities: vec![
                Entity {
                    name: "A".to_string(),
                    attributes: vec![],
                    relationships: vec![],
                },
                Entity {
                    name: "B".to_string(),
                    attributes: vec![],
                    relationships: vec![Relationship {
                        field_name: "a".to_string(),
                        target: "A".to_string(),
                        cardinality: Cardinality::One,
                    }],
                },
            ],
        };

        let edges = deduplicate(&schema);
        assert_eq!(edges.len(), 1);
        // original direction preserved: B→A
        assert_eq!(edges[0].left, "B");
        assert_eq!(edges[0].right, "A");
        assert_eq!(edges[0].left_to_right.field_name, "a");
        assert!(edges[0].right_to_left.is_none());
    }

    #[test]
    fn test_asymmetric_multiple_relationships() {
        // A has 2 edges to B, B has 1 edge to A
        let schema = Schema {
            entities: vec![
                Entity {
                    name: "A".to_string(),
                    attributes: vec![],
                    relationships: vec![
                        Relationship {
                            field_name: "primary_b".to_string(),
                            target: "B".to_string(),
                            cardinality: Cardinality::One,
                        },
                        Relationship {
                            field_name: "secondary_b".to_string(),
                            target: "B".to_string(),
                            cardinality: Cardinality::Many,
                        },
                    ],
                },
                Entity {
                    name: "B".to_string(),
                    attributes: vec![],
                    relationships: vec![Relationship {
                        field_name: "a".to_string(),
                        target: "A".to_string(),
                        cardinality: Cardinality::One,
                    }],
                },
            ],
        };

        let edges = deduplicate(&schema);
        assert_eq!(edges.len(), 2);

        // first pair is merged
        let merged = edges.iter().find(|e| e.right_to_left.is_some()).unwrap();
        assert_eq!(merged.left, "A");
        assert_eq!(merged.right, "B");

        // second is unidirectional
        let uni = edges.iter().find(|e| e.right_to_left.is_none()).unwrap();
        assert_eq!(uni.left, "A");
        assert_eq!(uni.right, "B");
    }

    #[test]
    fn test_empty_schema() {
        let schema = Schema { entities: vec![] };
        let edges = deduplicate(&schema);
        assert!(edges.is_empty());
    }

    #[test]
    fn test_no_relationships() {
        let schema = Schema {
            entities: vec![Entity {
                name: "Lonely".to_string(),
                attributes: vec![],
                relationships: vec![],
            }],
        };
        let edges = deduplicate(&schema);
        assert!(edges.is_empty());
    }
}
