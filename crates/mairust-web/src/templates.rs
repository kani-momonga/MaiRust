//! Template Engine
//!
//! Handles HTML template rendering using minijinja.

use minijinja::{context, Environment, Error as MiniJinjaError};
use std::collections::HashMap;

/// Template manager
pub struct Templates {
    env: Environment<'static>,
}

impl Templates {
    /// Create a new template manager
    pub fn new() -> Self {
        let mut env = Environment::new();

        // Register templates
        env.add_template("base", include_str!("../templates/base.html"))
            .expect("Failed to add base template");
        env.add_template("inbox", include_str!("../templates/inbox.html"))
            .expect("Failed to add inbox template");
        env.add_template("compose", include_str!("../templates/compose.html"))
            .expect("Failed to add compose template");
        env.add_template("message", include_str!("../templates/message.html"))
            .expect("Failed to add message template");
        env.add_template("settings", include_str!("../templates/settings.html"))
            .expect("Failed to add settings template");
        env.add_template("login", include_str!("../templates/login.html"))
            .expect("Failed to add login template");

        Self { env }
    }

    /// Render a template with context
    pub fn render(&self, name: &str, context: &serde_json::Value) -> Result<String, MiniJinjaError> {
        let template = self.env.get_template(name)?;
        template.render(context)
    }
}

impl Default for Templates {
    fn default() -> Self {
        Self::new()
    }
}
