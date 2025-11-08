//! Script runtime management

use rquickjs::{Context, Runtime};
use std::path::Path;

/// Script execution context
pub struct ScriptContext {
    #[allow(dead_code)] // Kept alive for context lifetime
    runtime: Runtime,
    context: Context,
}

impl ScriptContext {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let runtime = Runtime::new()?;
        let context = Context::full(&runtime)?;

        Ok(Self { runtime, context })
    }

    pub fn execute_file(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let source = std::fs::read_to_string(path)?;
        self.execute(&source)?;
        Ok(())
    }

    pub fn execute(&self, source: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.context.with(|ctx| {
            ctx.eval::<(), _>(source)?;
            Ok::<_, rquickjs::Error>(())
        })?;
        Ok(())
    }
}
