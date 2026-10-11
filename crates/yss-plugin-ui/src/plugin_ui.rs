//! Plugin management and native views using the process-wide plugin manager.
pub mod plugins;

use std::sync::Arc;
use tokio::runtime::Handle;
use yss_plugin_runtime::PluginManager;

/// The executor and canonical plugin manager needed by plugin presentation.
pub struct PluginServices {
    executor: Handle,
    plugins: PluginManager,
}

impl PluginServices {
    pub fn new(executor: Handle, plugins: PluginManager) -> Self {
        Self { executor, plugins }
    }

    fn run<T: Send + 'static>(
        self: &Arc<Self>,
        operation: impl FnOnce(&Self) -> anyhow::Result<T> + Send + 'static,
    ) -> tokio::task::JoinHandle<anyhow::Result<T>> {
        let owner = self.clone();
        self.executor.spawn_blocking(move || operation(&owner))
    }
}
