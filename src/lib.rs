pub mod config;
pub mod brains;
pub mod bridges;
pub mod conductor;
pub mod tools;

// Re-export gemini for backward compatibility during refactor if needed, 
// or simply expose the new path.
// For now, let's expose gemini from its new home for the integration tests.
pub use brains::gemini;
