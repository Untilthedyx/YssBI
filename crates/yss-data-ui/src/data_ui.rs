pub mod databases;
pub mod imports;

use std::sync::Arc;
use yss_application::database::samples::SampleCatalog;
use yss_desktop_services::ApplicationTasks;

/// Import discovery capabilities; the application remains the dataset authority.
pub struct DataServices {
    tasks: Arc<ApplicationTasks>,
    samples: SampleCatalog,
}

impl DataServices {
    pub fn new(tasks: Arc<ApplicationTasks>, samples: SampleCatalog) -> Self {
        Self { tasks, samples }
    }
}
