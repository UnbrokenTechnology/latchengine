//! Script runtime management
//!
//! Provides a JavaScript runtime for game logic execution.
//! For the PoC, we keep it simple and expose FFI via manual injection.

use rquickjs::{Context, Runtime};
use std::path::Path;

/// Script execution context
pub struct ScriptRuntime {
    #[allow(dead_code)] // Kept alive for context lifetime
    runtime: Runtime,
    pub context: Context,
}

impl ScriptRuntime {
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

    /// Call a JavaScript function by name with no arguments.
    pub fn call_function(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.context
            .with(|ctx| -> Result<(), Box<dyn std::error::Error>> {
                let globals = ctx.globals();
                let func: rquickjs::Function = globals.get(name)?;
                func.call::<_, ()>(())?;
                Ok(())
            })?;
        Ok(())
    }
}

impl Default for ScriptRuntime {
    fn default() -> Self {
        Self::new().expect("Failed to create script runtime")
    }
}
