pub mod rust;

use super::extractor::LanguageExtractorRegistry;

/// Registers every implemented language extractor. Called once when building
/// the indexer (Task 8). Each language task in this plan adds one line here.
#[allow(dead_code)]
pub fn register_all(registry: &mut LanguageExtractorRegistry) {
    registry.register(Box::new(rust::RustExtractor));
}
