use apollo_federation::composition::{
    pre_merge_validations, post_merge_validations, CompositionOptions
};
use apollo_federation::subgraph::typestate::{Subgraph, Initial, Validated};
use apollo_federation::supergraph::{Supergraph, Merged};
use apollo_compiler::Schema;


#[cfg(test)]
mod composition_tests {
    use super::*;

    /// Test the pre_merge_validations function directly
    #[test]
    fn test_pre_merge_validations_empty_subgraphs() {
        let empty_subgraphs: Vec<Subgraph<Validated>> = vec![];
        let result = pre_merge_validations(&empty_subgraphs);
        
        match result {
            Ok(_) => panic!("Should fail with empty subgraphs"),
            Err(errors) => {
                assert!(!errors.is_empty(), "Should have at least one error");
                let error_messages: Vec<String> = errors.iter()
                    .map(|e| e.to_string())
                    .collect();
                assert!(
                    error_messages.iter().any(|msg| msg.contains("empty subgraphs")),
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
        
        let subgraph = Subgraph::<Initial>::parse("test", "http://localhost:4000", schema_str)
            .expect("Failed to parse subgraph")
            .expand_links()
            .expect("Failed to expand links");
            
        // Need to upgrade before validating
        let upgraded_subgraphs = apollo_federation::composition::upgrade_subgraphs_if_necessary(vec![subgraph])
            .expect("Failed to upgrade subgraph");
            
        let validated_subgraph = upgraded_subgraphs.into_iter().next()
            .expect("Should have one subgraph")
            .validate()
            .expect("Failed to validate subgraph");
            
        let subgraphs = vec![validated_subgraph];
        let result = pre_merge_validations(&subgraphs);
        
        assert!(result.is_ok(), "pre_merge_validations should succeed with valid subgraphs");
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
        
        let schema = Schema::parse_and_validate(schema_str, "test.graphql")
            .expect("Failed to parse schema");
        let supergraph = Supergraph::<Merged>::new(schema);
        
        let result = post_merge_validations(&supergraph);
        assert!(result.is_ok(), "post_merge_validations should succeed with valid supergraph");
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
        
        let schema = Schema::parse_and_validate(schema_str, "test.graphql")
            .expect("Failed to parse schema");
        let supergraph = Supergraph::<Merged>::new(schema);
        
        let result = post_merge_validations(&supergraph);
        assert!(result.is_ok(), "post_merge_validations should succeed with comprehensive valid supergraph");
    }

    /// Test CompositionOptions structure
    #[test]
    fn test_composition_options() {
        // Test default options
        let default_options = CompositionOptions::default();
        assert!(default_options.run_satisfiability, "Default should enable satisfiability");
        
        // Test custom options
        let custom_options = CompositionOptions {
            run_satisfiability: false,
        };
        assert!(!custom_options.run_satisfiability, "Custom options should work");
    }
}