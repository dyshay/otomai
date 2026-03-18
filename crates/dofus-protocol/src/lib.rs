pub mod generated;

// Re-export generated modules
pub use generated::types;

// Compatibility wrappers matching handler import paths
pub mod messages;
pub mod enums;
pub mod registry;
