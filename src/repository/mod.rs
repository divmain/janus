//! Generic repository pattern for entity queries

use crate::error::Result;

/// Generic repository trait for batch operations
#[async_trait::async_trait]
pub trait ItemRepository: Send + Sync {
    type Item;
    type Metadata: HasId + Clone + Send + Sync;

    /// Get all items from cache or disk (static version for zero-sized types)
    async fn get_all_static() -> Result<Vec<Self::Metadata>> {
        panic!("get_all_static must be implemented by concrete types")
    }

    /// Get all items from cache or disk (instance version)
    async fn get_all(&self) -> Result<Vec<Self::Metadata>> {
        Self::get_all_static().await
    }

    /// Build a HashMap by ID (static version)
    async fn build_map_static() -> Result<std::collections::HashMap<String, Self::Metadata>> {
        let items = Self::get_all_static().await?;
        let map: std::collections::HashMap<_, _> = items
            .into_iter()
            .filter_map(|m| m.get_id().map(|id| (id, m)))
            .collect();
        Ok(map)
    }

    /// Build a HashMap by ID (instance version)
    async fn build_map(&self) -> Result<std::collections::HashMap<String, Self::Metadata>> {
        Self::build_map_static().await
    }

    /// Get all items and the map together (efficient single call)
    async fn get_all_with_map(&self) -> Result<(Vec<Self::Metadata>, std::collections::HashMap<String, Self::Metadata>)> {
        let items = self.get_all().await?;
        let map: std::collections::HashMap<_, _> = items
            .iter()
            .filter_map(|m| m.get_id().map(|id| (id.clone(), m.clone())))
            .collect();
        Ok((items, map))
    }

    /// Get all items and the map together (static version)
    async fn get_all_with_map_static() -> Result<(Vec<Self::Metadata>, std::collections::HashMap<String, Self::Metadata>)> {
        let items = Self::get_all_static().await?;
        let map: std::collections::HashMap<_, _> = items
            .iter()
            .filter_map(|m| m.get_id().map(|id| (id.clone(), m.clone())))
            .collect();
        Ok((items, map))
    }
}

/// Trait for types that can provide an ID
pub trait HasId {
    fn get_id(&self) -> Option<String>;
}

