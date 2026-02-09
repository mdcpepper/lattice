//! Item Fixtures

use serde::Deserialize;

/// Wrapper for items in YAML
#[derive(Debug, Deserialize)]
pub struct ItemsFixture {
    /// Vector of product key references
    pub items: Vec<String>,
}
