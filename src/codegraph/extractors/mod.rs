pub mod javascript;
pub mod python;
pub mod rust;
pub mod typescript;

use super::extractor::LanguageExtractorRegistry;

/// Registers every implemented language extractor. Called once when building
/// the indexer (Task 8). Each language task in this plan adds one line here.
#[allow(dead_code)]
pub fn register_all(registry: &mut LanguageExtractorRegistry) {
    registry.register(Box::new(javascript::JavaScriptExtractor));
    registry.register(Box::new(python::PythonExtractor));
    registry.register(Box::new(rust::RustExtractor));
    registry.register(Box::new(typescript::TypeScriptExtractor));
    registry.register(Box::new(typescript::TsxExtractor));
}
