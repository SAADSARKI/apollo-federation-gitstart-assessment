mod satisfiability;

use std::collections::HashSet;
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

/// Options for composition
#[derive(Debug, Clone)]
pub struct CompositionOptions {
    /// Whether to run satisfiability validation (defaults to true)
    pub run_satisfiability: bool,
}

impl Default for CompositionOptions {
    fn default() -> Self {
        Self {
            run_satisfiability: true,
        }
    }
}

/// Main compose function
pub fn compose(
    subgraphs: Vec<Subgraph<Initial>>,
) -> Result<Supergraph<Satisfiable>, Vec<CompositionError>> {
    compose_with_options(subgraphs, CompositionOptions::default())
}

/// Compose with options support
pub fn compose_with_options(
    subgraphs: Vec<Subgraph<Initial>>,
    options: CompositionOptions,
) -> Result<Supergraph<Satisfiable>, Vec<CompositionError>> {
    let expanded_subgraphs = expand_subgraphs(subgraphs)?;
    let upgraded_subgraphs = upgrade_subgraphs_if_necessary(expanded_subgraphs)?;
    let validated_subgraphs = validate_subgraphs(upgraded_subgraphs)?;

    pre_merge_validations(&validated_subgraphs)?;
    let supergraph = merge_subgraphs(validated_subgraphs)?;
    post_merge_validations(&supergraph)?;

    if options.run_satisfiability {
        validate_satisfiability(supergraph)
    } else {
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
    if subgraphs.is_empty() {
        return Err(vec![CompositionError::InternalError {
            message: "Cannot compose with empty subgraphs list".to_string(),
        }]);
    }
    
    // Check for duplicate subgraph names
    let mut seen_names = HashSet::new();
    for subgraph in subgraphs {
        if !seen_names.insert(&subgraph.name) {
            return Err(vec![CompositionError::InternalError {
                message: format!("Duplicate subgraph name: {}", subgraph.name),
            }]);
        }
    }
    
    // TODO: Add more cross-subgraph validations:
    // - Check @key fields exist and are valid
    // - Validate @provides/@requires consistency
    // - Check for type conflicts across subgraphs
    
    Ok(())
}

pub fn merge_subgraphs(
    subgraphs: Vec<Subgraph<Validated>>,
) -> Result<Supergraph<Merged>, Vec<CompositionError>> {
    use crate::merger::merge::CompositionOptions as MergerOptions;
    use crate::merger::merge::merge_subgraphs as new_merge_subgraphs;

    let options = MergerOptions::default();
    let merge_result = new_merge_subgraphs(subgraphs, options).map_err(|e| {
        vec![CompositionError::InternalError {
            message: format!("Merge failed: {}", e),
        }]
    })?;

    if !merge_result.errors.is_empty() {
        return Err(merge_result.errors);
    }

    if let Some(supergraph_schema) = merge_result.supergraph {
        // Need to convert from Valid<FederationSchema> to Valid<Schema>
        let schema = supergraph_schema.into_inner().into_inner();
        let valid_schema = apollo_compiler::validation::Valid::assume_valid(schema);

        let supergraph = Supergraph::<Merged>::new(valid_schema);
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
    let schema = supergraph.schema();
    let mut errors = Vec::new();

    // Check query type exists
    if schema.schema_definition.query.is_none() {
        errors.push(CompositionError::TypeDefinitionInvalid {
            message: "Supergraph must have a query type".to_string(),
        });
    }

    // Check for _Entity union if there are entities
    if let Some(entity_type) = schema.types.get("_Entity") {
        if let apollo_compiler::schema::ExtendedType::Union(union_type) = entity_type {
            if union_type.members.is_empty() {
                errors.push(CompositionError::TypeDefinitionInvalid {
                    message: "_Entity union exists but has no members".to_string(),
                });
            }
        }
    }

    // TODO: Add more validations:
    // - Check for orphaned types (not reachable from Query/Mutation/Subscription)
    // - Validate all @key directives are valid
    // - Check entity consistency

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
