use html_minifier::HTMLMinifier;
use tera::Context;
use thiserror::Error;
use tide::log::error;

use crate::State;

#[derive(Error, Debug)]
pub enum TemplateError {
    #[error("rendering error")]
    Tera(#[from] tera::Error),
}

pub(super) fn render(
    state: &State,
    template: &'static str,
    context: &Context,
) -> Result<String, TemplateError> {
    let rendered = state.tera.render(template, context)?;

    let mut html_minifier = HTMLMinifier::new();
    if let Err(err) = html_minifier.digest(&rendered) {
        error!("Failed to minify HTML: {}", err);
        return Ok(rendered);
    };

    let minified = match std::str::from_utf8(html_minifier.get_html()) {
        Ok(minified) => minified.to_string(),
        Err(err) => {
            error!("Failed to parse minified HTML as UTF-8: {}", err);
            rendered
        },
    };

    Ok(minified)
}
