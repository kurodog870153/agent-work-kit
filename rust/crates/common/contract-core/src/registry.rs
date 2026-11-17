//! Per-domain registry of public contracts and command trees.
//!
//! A registry is built from resources a domain owns. `contract-core` performs
//! structural validation and answers contract queries; it never carries a
//! second copy of a domain contract and never parses domain command input.

use std::collections::BTreeMap;

use crate::error::{ContractError, ContractResult};
use crate::model::{CommandNode, ContractRecord, Example};
use crate::validation;

/// Resources one domain supplies to build its own registry.
#[derive(Debug, Clone)]
pub struct DomainResources {
    pub domain: String,
    pub root_commands: Vec<CommandNode>,
    pub contracts: Vec<ContractRecord>,
    pub examples: BTreeMap<String, BTreeMap<String, Example>>,
}

/// Indexed command entry. `path` is the space-separated command path.
#[derive(Debug, Clone)]
pub struct CommandEntry {
    pub path: String,
    pub node: CommandNode,
}

/// Validated, immutable view of one domain's public contract surface.
#[derive(Debug, Clone)]
pub struct Registry {
    domain: String,
    commands: BTreeMap<String, CommandEntry>,
    contracts: BTreeMap<String, ContractRecord>,
    examples: BTreeMap<String, BTreeMap<String, Example>>,
}

impl Registry {
    /// Build a registry, rejecting any structural inconsistency.
    pub fn build(resources: DomainResources) -> ContractResult<Self> {
        let mut registry = Self {
            domain: resources.domain,
            commands: BTreeMap::new(),
            contracts: resources
                .contracts
                .into_iter()
                .map(|record| (record.description.id.clone(), record))
                .collect(),
            examples: resources.examples,
        };
        for root in &resources.root_commands {
            registry.index_command(root, "")?;
        }
        validation::validate(&registry)?;
        Ok(registry)
    }

    fn index_command(&mut self, node: &CommandNode, parent: &str) -> ContractResult<()> {
        let path = if parent.is_empty() {
            node.name.clone()
        } else {
            format!("{parent} {}", node.name)
        };
        if self
            .commands
            .insert(
                path.clone(),
                CommandEntry {
                    path: path.clone(),
                    node: node.clone(),
                },
            )
            .is_some()
        {
            return Err(ContractError::new(
                "duplicate_command_path",
                format!("Duplicate command path: {path}"),
            ));
        }
        for child in &node.children {
            self.index_command(child, &path)?;
        }
        Ok(())
    }

    pub fn domain(&self) -> &str {
        &self.domain
    }

    pub fn command(&self, path: &str) -> Option<&CommandEntry> {
        self.commands.get(path)
    }

    pub fn commands(&self) -> impl Iterator<Item = &CommandEntry> {
        self.commands.values()
    }

    pub fn root_commands(&self) -> impl Iterator<Item = &CommandEntry> {
        self.commands
            .values()
            .filter(|entry| !entry.path.contains(' '))
    }

    pub fn contract(&self, id: &str) -> Option<&ContractRecord> {
        self.contracts.get(id)
    }

    pub fn contracts(&self) -> impl Iterator<Item = &ContractRecord> {
        self.contracts.values()
    }

    pub fn example(&self, id: &str, scenario: &str) -> Option<&Example> {
        self.examples.get(id)?.get(scenario)
    }

    pub fn scenarios(&self, id: &str) -> Vec<&str> {
        self.examples
            .get(id)
            .map(|items| items.keys().map(String::as_str).collect())
            .unwrap_or_default()
    }

    pub(crate) fn examples_index(&self) -> &BTreeMap<String, BTreeMap<String, Example>> {
        &self.examples
    }
}
