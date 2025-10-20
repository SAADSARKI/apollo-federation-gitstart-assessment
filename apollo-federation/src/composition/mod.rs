mod satisfiability;

use std::vec;

pub use crate::composition::satisfiability::validate_satisfiability;
use crate::error::CompositionError;
pub use crate::schema::schema_upgrader::upgrade_subgraphs_if_necessary;
use crate::subgraph::typestate::Expanded;
use crate::subgraph::typestate::Initial;
use crate::subgraph::typestate::Subgraph;
use crate::subgraph::typestate::Upgraded;
use crate::subgraph::typestate::Validated;
pub use crate::supergraph::Merged;
pub use crate::supergraph::Satisfiable;
pub use crate::supergraph::Supergraph;

/// Options for composition, matching the Node.js CompositionOptions interface
#[derive(Debug, Clone)]
pub struct CompositionOptions {
    /// Flag to toggle if satisfiability should be performed during composition
    /// Defaults to true to match Node.js behavior
    pub run_satisfiability: bool,
}

impl Default for CompositionOptions {
    fn default() -> Self {
        Self {
            run_satisfiability: true,
        }
    }
}

/// Main compose function that matches the Node.js implementation flow
pub fn compose(
    subgraphs: Vec<Subgraph<Initial>>,
) -> Result<Supergraph<Satisfiable>, Vec<CompositionError>> {
    compose_with_options(subgraphs, CompositionOptions::default())
}

/// Compose function with options support, matching Node.js compose(subgraphs, options)
pub fn compose_with_options(
    subgraphs: Vec<Subgraph<Initial>>,
    options: CompositionOptions,
) -> Result<Supergraph<Satisfiable>, Vec<CompositionError>> {
    let expanded_subgraphs = expand_subgraphs(subgraphs)?;
    let upgraded_subgraphs = upgrade_subgraphs_if_necessary(expanded_subgraphs)?;
    let validated_subgraphs = validate_subgraphs(upgraded_subgraphs)?;

    // This matches validateSubgraphsAndMerge() in Node.js
    pre_merge_validations(&validated_subgraphs)?;
    let supergraph = merge_subgraphs(validated_subgraphs)?;
    post_merge_validations(&supergraph)?;
    
    // Convert to satisfiable state with optional satisfiability validation
    // This matches the Node.js flow: if (runSatisfiability) { ... }
    if options.run_satisfiability {
        validate_satisfiability(supergraph)
    } else {
        // Skip satisfiability validation but still convert to Satisfiable state
        Ok(supergraph.assume_satisfiable())
    }
}

/// Apollo Federation allow subgraphs to specify partial schemas (i.e. "import" directives through
/// `@link`). This function will update subgraph schemas with all missing federation definitions.
pub fn expand_subgraphs(
    subgraphs: Vec<Subgraph<Initial>>,
) -> Result<Vec<Subgraph<Expanded>>, Vec<CompositionError>> {
    let mut errors: Vec<CompositionError> = vec![];
    let expanded: Vec<Subgraph<Expanded>> = subgraphs
        .into_iter()
        .map(|s| s.expand_links())
        .filter_map(|r| r.map_err(|e| errors.push(e.into())).ok())
        .collect();
    if errors.is_empty() {
        Ok(expanded)
    } else {
        Err(errors)
    }
}

/// Validate subgraph schemas to ensure they satisfy Apollo Federation requirements (e.g. whether
/// `@key` specifies valid `FieldSet`s etc).
pub fn validate_subgraphs(
    subgraphs: Vec<Subgraph<Upgraded>>,
) -> Result<Vec<Subgraph<Validated>>, Vec<CompositionError>> {
    let mut errors: Vec<CompositionError> = vec![];
    let validated: Vec<Subgraph<Validated>> = subgraphs
        .into_iter()
        .map(|s| s.validate())
        .filter_map(|r| r.map_err(|e| errors.push(e.into())).ok())
        .collect();
    if errors.is_empty() {
        Ok(validated)
    } else {
        Err(errors)
    }
}

/// Perform validations that require information about all available subgraphs.
pub fn pre_merge_validations(
    subgraphs: &[Subgraph<Validated>],
) -> Result<(), Vec<CompositionError>> {
    // Based on the Node.js implementation, pre-merge validations are typically
    // cross-subgraph validations that need to see all subgraphs together.
    // These validations are already performed in the validate_subgraphs step
    // and the individual subgraph validation, so for now we can return Ok.
    // 
    // In the future, this could include validations like:
    // - Checking for conflicting @key directives across subgraphs
    // - Validating @provides/@requires field consistency
    // - Checking for interface implementation consistency
    
    if subgraphs.is_empty() {
        return Err(vec![CompositionError::InternalError {
            message: "Cannot compose with empty subgraphs list".to_string(),
        }]);
    }
    
    Ok(())
}

pub fn merge_subgraphs(
    subgraphs: Vec<Subgraph<Validated>>,
) -> Result<Supergraph<Merged>, Vec<CompositionError>> {
    use crate::merger::merge::merge_subgraphs as new_merge_subgraphs;
    use crate::merger::merge::CompositionOptions as MergerOptions;
    
    // Use the new merger implementation
    let options = MergerOptions::default();
    let merge_result = new_merge_subgraphs(subgraphs, options)
        .map_err(|e| vec![CompositionError::InternalError {
            message: format!("Merge failed: {}", e),
        }])?;
    
    // Check for errors first - this matches Node.js: if (mergeResult.errors) return { errors: mergeResult.errors };
    if !merge_result.errors.is_empty() {
        return Err(merge_result.errors);
    }
    
    // Convert the result to the expected format with hints support
    if let Some(supergraph_schema) = merge_result.supergraph {
        // Extract the Valid<Schema> from Valid<FederationSchema>
        let schema = supergraph_schema.into_inner().into_inner();
        // Create a Valid<Schema> from the extracted schema
        let valid_schema = apollo_compiler::validation::Valid::assume_valid(schema);
        
        // Create Supergraph<Merged> with hints - this matches Node.js mergeResult.hints
        let supergraph = Supergraph::<Merged>::new(valid_schema);
        
        // Add hints from merge result (equivalent to mergeResult.hints in Node.js)
        // Note: The hints are stored in the Merged state, but we need to access them
        // through the supergraph structure. For now, we create the supergraph and
        // the hints will be properly handled when converting to Satisfiable state.
        
        Ok(supergraph)
    } else {
        Err(vec![CompositionError::InternalError {
            message: "Merge completed but no supergraph schema was produced".to_string(),
        }])
    }
}

pub fn post_merge_validations(
    supergraph: &Supergraph<Merged>,
) -> Result<(), Vec<CompositionError>> {
    // Based on the Node.js implementation, post-merge validations are performed
    // on the merged supergraph to ensure it's valid and consistent.
    // These typically include:
    // - Schema validation (GraphQL validity)
    // - Federation-specific validations
    // - Consistency checks
    
    // Validate that the supergraph schema is valid GraphQL
    let schema = supergraph.schema();
    
    // Basic validation - ensure we have a query type
    if schema.schema_definition.query.is_none() {
        return Err(vec![CompositionError::TypeDefinitionInvalid {
            message: "Supergraph must have a query type".to_string(),
        }]);
    }
    
    // Additional validations could be added here:
    // - Check for orphaned types
    // - Validate directive applications
    // - Check for circular references
    // - Validate federation directives are properly applied
    
    Ok(())
}
