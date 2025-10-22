use apollo_compiler::Schema;
use apollo_federation::Supergraph;
use apollo_federation::composition::{
    CompositionOptions, post_merge_validations, pre_merge_validations,
};
use apollo_federation::subgraph::Subgraph;
use apollo_federation::subgraph::typestate::{Initial, Subgraph as TypestateSubgraph, Validated};
use apollo_federation::supergraph::{Merged, Supergraph as TypestateSupergraph};

fn print_sdl(schema: &Schema) -> String {
    let mut schema = schema.clone();
    schema.types.sort_keys();
    schema.directive_definitions.sort_keys();
    schema.to_string()
}

#[test]
fn can_compose_supergraph() {
    let s1 = Subgraph::parse_and_expand(
        "Subgraph1",
        "https://subgraph1",
        r#"
        type Query {
            t: T
        }
        
        type T @key(fields: "k") {
            k: ID
        }
        
        type S {
            x: Int
        }
        
        union U = S | T
        "#,
    )
    .unwrap();

    let s2 = Subgraph::parse_and_expand(
        "Subgraph2",
        "https://subgraph2",
        r#"
        type T @key(fields: "k") {
            k: ID
            a: Int
            b: String
        }
        
        enum E {
            V1
            V2
        }
        "#,
    )
    .unwrap();

    let supergraph = Supergraph::compose(vec![&s1, &s2]).unwrap();
    insta::assert_snapshot!(print_sdl(supergraph.schema.schema()));
    insta::assert_snapshot!(print_sdl(
        supergraph
            .to_api_schema(Default::default())
            .unwrap()
            .schema()
    ));
}

#[test]
fn can_compose_with_descriptions() {
    let s1 = Subgraph::parse_and_expand(
        "Subgraph1",
        "https://subgraph1",
        r#"
        "The foo directive description"
        directive @foo(url: String) on FIELD
        
        "A cool schema"
        schema {
            query: Query
        }
        
        """
        Available queries
        Not much yet
        """
        type Query {
            "Returns tea"
            t("An argument that is very important" x: String!): String
        }
        "#,
    )
    .unwrap();

    let s2 = Subgraph::parse_and_expand(
        "Subgraph2",
        "https://subgraph2",
        r#"
        "The foo directive description" 
        directive @foo(url: String) on FIELD
        
        "An enum"
        enum E {
            "The A value"
            A
            "The B value" 
            B
        }
        "#,
    )
    .unwrap();

    let supergraph = Supergraph::compose(vec![&s1, &s2]).unwrap();
    insta::assert_snapshot!(print_sdl(supergraph.schema.schema()));
    insta::assert_snapshot!(print_sdl(
        supergraph
            .to_api_schema(Default::default())
            .unwrap()
            .schema()
    ));
}

#[test]
fn can_compose_types_from_different_subgraphs() {
    let s1 = Subgraph::parse_and_expand(
        "SubgraphA",
        "https://subgraphA",
        r#"
        type Query {
            products: [Product!]
        }
        
        type Product {
            sku: String!
            name: String!
        }
        "#,
    )
    .unwrap();

    let s2 = Subgraph::parse_and_expand(
        "SubgraphB",
        "https://subgraphB",
        r#"
        type User {
            name: String
            email: String!
        }
        "#,
    )
    .unwrap();

    let supergraph = Supergraph::compose(vec![&s1, &s2]).unwrap();
    insta::assert_snapshot!(print_sdl(supergraph.schema.schema()));
    insta::assert_snapshot!(print_sdl(
        supergraph
            .to_api_schema(Default::default())
            .unwrap()
            .schema()
    ));
}

#[test]
fn compose_removes_federation_directives() {
    let s1 = Subgraph::parse_and_expand(
        "SubgraphA",
        "https://subgraphA",
        r#"
        extend schema @link(url: "https://specs.apollo.dev/federation/v2.5", import: [ "@key", "@provides", "@external" ])
        
        type Query {
            products: [Product!] @provides(fields: "name")
        }
        
        type Product @key(fields: "sku") {
            sku: String!
            name: String! @external
        }
        "#,
    ).unwrap();

    let s2 = Subgraph::parse_and_expand(
        "SubgraphB",
        "https://subgraphB",
        r#"
        extend schema @link(url: "https://specs.apollo.dev/federation/v2.5", import: [ "@key", "@shareable" ])
        
        type Product @key(fields: "sku") {
            sku: String!
            name: String! @shareable
        }
        "#,
    ).unwrap();

    let supergraph = Supergraph::compose(vec![&s1, &s2]).unwrap();
    insta::assert_snapshot!(print_sdl(supergraph.schema.schema()));
    insta::assert_snapshot!(print_sdl(
        supergraph
            .to_api_schema(Default::default())
            .unwrap()
            .schema()
    ));
}

// Unit tests for the individual functions
#[cfg(test)]
mod unit_tests {
    use super::*;

    /// Test the pre_merge_validations function directly
    #[test]
    fn test_pre_merge_validations_empty_subgraphs() {
        let empty_subgraphs: Vec<TypestateSubgraph<Validated>> = vec![];
        let result = pre_merge_validations(&empty_subgraphs);

        match result {
            Ok(_) => panic!("Should fail with empty subgraphs"),
            Err(errors) => {
                assert!(!errors.is_empty(), "Should have at least one error");
                let error_messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
                assert!(
                    error_messages
                        .iter()
                        .any(|msg| msg.contains("empty subgraphs")),
                    "Should contain error about empty subgraphs, got: {:?}",
                    error_messages
                );
            }
        }
    }

    /// Test the pre_merge_validations function with valid subgraphs
    #[test]
    fn test_pre_merge_validations_success() {
        // Create a mock validated subgraph for testing
        let schema_str = r#"
            type Query {
                hello: String
            }
        "#;

        let subgraph =
            TypestateSubgraph::<Initial>::parse("test", "http://localhost:4000", schema_str)
                .expect("Failed to parse subgraph")
                .expand_links()
                .expect("Failed to expand links");

        // Need to upgrade before validating
        let upgraded_subgraphs =
            apollo_federation::composition::upgrade_subgraphs_if_necessary(vec![subgraph])
                .expect("Failed to upgrade subgraph");

        let validated_subgraph = upgraded_subgraphs
            .into_iter()
            .next()
            .expect("Should have one subgraph")
            .validate()
            .expect("Failed to validate subgraph");

        let subgraphs = vec![validated_subgraph];
        let result = pre_merge_validations(&subgraphs);

        assert!(
            result.is_ok(),
            "pre_merge_validations should succeed with valid subgraphs"
        );
    }

    /// Test the post_merge_validations function
    #[test]
    fn test_post_merge_validations_success() {
        // Create a simple valid schema for testing
        let schema_str = r#"
            type Query {
                hello: String
            }
        "#;

        let schema =
            Schema::parse_and_validate(schema_str, "test.graphql").expect("Failed to parse schema");
        let supergraph = TypestateSupergraph::<Merged>::new(schema);

        let result = post_merge_validations(&supergraph);
        assert!(
            result.is_ok(),
            "post_merge_validations should succeed with valid supergraph"
        );
    }

    /// Test the post_merge_validations function with a valid schema that has all required types
    #[test]
    fn test_post_merge_validations_comprehensive() {
        // Create a more comprehensive schema to test validation
        let schema_str = r#"
            type Query {
                user(id: ID!): User
                users: [User!]!
            }
            
            type Mutation {
                createUser(input: UserInput!): User
            }
            
            type User {
                id: ID!
                name: String!
                email: String
            }
            
            input UserInput {
                name: String!
                email: String
            }
        "#;

        let schema =
            Schema::parse_and_validate(schema_str, "test.graphql").expect("Failed to parse schema");
        let supergraph = TypestateSupergraph::<Merged>::new(schema);

        let result = post_merge_validations(&supergraph);
        assert!(
            result.is_ok(),
            "post_merge_validations should succeed with comprehensive valid supergraph"
        );
    }

    /// Test CompositionOptions structure
    #[test]
    fn test_composition_options() {
        // Test default options
        let default_options = CompositionOptions::default();
        assert!(
            default_options.run_satisfiability,
            "Default should enable satisfiability"
        );

        // Test custom options
        let custom_options = CompositionOptions {
            run_satisfiability: false,
        };
        assert!(
            !custom_options.run_satisfiability,
            "Custom options should work"
        );
    }

    /// Test duplicate subgraph names validation
    #[test]
    fn test_pre_merge_validations_duplicate_names() {
        // Create two subgraphs with the same name
        let schema_str = r#"
            type Query {
                hello: String
            }
        "#;

        let subgraph1 =
            TypestateSubgraph::<Initial>::parse("duplicate", "http://localhost:4000", schema_str)
                .expect("Failed to parse subgraph1")
                .expand_links()
                .expect("Failed to expand links");

        let subgraph2 =
            TypestateSubgraph::<Initial>::parse("duplicate", "http://localhost:4001", schema_str)
                .expect("Failed to parse subgraph2")
                .expand_links()
                .expect("Failed to expand links");

        // Upgrade both subgraphs
        let upgraded_subgraphs =
            apollo_federation::composition::upgrade_subgraphs_if_necessary(vec![
                subgraph1, subgraph2,
            ])
            .expect("Failed to upgrade subgraphs");

        let validated_subgraphs: Vec<TypestateSubgraph<Validated>> = upgraded_subgraphs
            .into_iter()
            .map(|s| s.validate().expect("Failed to validate subgraph"))
            .collect();

        let result = pre_merge_validations(&validated_subgraphs);

        match result {
            Ok(_) => panic!("Should fail with duplicate subgraph names"),
            Err(errors) => {
                assert!(!errors.is_empty(), "Should have at least one error");
                let error_messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
                assert!(
                    error_messages
                        .iter()
                        .any(|msg| msg.contains("Duplicate subgraph name")),
                    "Should contain error about duplicate subgraph names, got: {:?}",
                    error_messages
                );
            }
        }
    }
}
