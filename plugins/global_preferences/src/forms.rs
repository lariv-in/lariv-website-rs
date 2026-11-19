//! Global preferences form.

use lariv_core::html_form::{
    html_form,
    widgets::{Section, Text},
};

#[html_form(default)]
pub struct PreferencesForm {
    #[form(widget = Section, label = "Default API keys")]
    _section_keys: (),

    #[form(label = "Gemini API key", widget = Text)]
    pub gemini_api_key: String,

    #[form(label = "Google Search API key", widget = Text)]
    pub google_search_api_key: String,

    #[form(label = "Google Search CX id", widget = Text)]
    pub google_search_cx: String,
}
