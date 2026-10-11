//! Retained conversation presentation and project-scoped conversation operations.
mod assistant;
mod controller;

pub use assistant::{CancelResponse, ConversationPanel, principal};
pub use controller::{AssistantController, AssistantEvent, AssistantEventKind, DirectoryReadState};

use std::sync::Arc;
use yss_application::harness::knowledge::ProjectKnowledgeService;
use yss_application::harness::models::LanguageModelService;
use yss_desktop_services::ApplicationTasks;
use yss_harness_core::HarnessHost;

/// Canonical application capabilities used by conversation UI, not a second session authority.
pub struct AssistantServices {
    tasks: Arc<ApplicationTasks>,
    host: Arc<HarnessHost>,
    models: Arc<LanguageModelService>,
    knowledge: Arc<ProjectKnowledgeService>,
}

impl AssistantServices {
    pub fn new(
        tasks: Arc<ApplicationTasks>,
        host: Arc<HarnessHost>,
        models: Arc<LanguageModelService>,
        knowledge: Arc<ProjectKnowledgeService>,
    ) -> Self {
        Self {
            tasks,
            host,
            models,
            knowledge,
        }
    }
}
