#[derive(Debug)]
pub enum LanguageModel {
    Ollama,
}

#[derive(Debug)]
pub struct LanguageModelProvider {
    model: LanguageModel,
}

impl LanguageModelProvider {
    pub fn new(model: LanguageModel) -> Self {
        Self { model }
    }
}
