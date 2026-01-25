//! Generic repository pattern for entity queries

use crate::error::Result;

/// Generic repository trait for batch operations (zero-sized types only)
///
/// This trait is designed for zero-sized types that act as static namespaces
/// for entity queries. All methods are async static methods that operate on
/// the type itself rather than on instances.
#[async_trait::async_trait]
pub trait ItemRepository: Send + Sync {
    type Item;
    type Metadata: HasId + Clone + Send + Sync;

    /// Get all items from cache or disk
    ///
    /// Concrete types must implement this method.
    async fn get_all() -> Result<Vec<Self::Metadata>>;

    /// Build a HashMap by ID from all items
    ///
    /// Default implementation uses `get_all()` and builds a map.
    /// Types can override for optimized implementations.
    async fn build_map() -> Result<std::collections::HashMap<String, Self::Metadata>> {
        let items = Self::get_all().await?;
        let map: std::collections::HashMap<_, _> = items
            .into_iter()
            .filter_map(|m| m.get_id().map(|id| (id, m)))
            .collect();
        Ok(map)
    }

    /// Get all items and the map together (efficient single call)
    ///
    /// Default implementation calls `get_all()` and builds both the vec and map.
    /// Types can override for optimized implementations that avoid duplication.
    async fn get_all_with_map() -> Result<(
        Vec<Self::Metadata>,
        std::collections::HashMap<String, Self::Metadata>,
    )> {
        let items = Self::get_all().await?;
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
