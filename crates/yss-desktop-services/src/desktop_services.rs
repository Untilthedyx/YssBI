pub mod resources;

use std::sync::Arc;

use anyhow::Result;
use tokio::{runtime::Handle, sync::broadcast, task::JoinHandle};
use yss_application::{events::CommittedResourceMutation, session::ApplicationState};
use yss_harness_contract::HarnessEventEnvelope;
use yss_project_identity::ProjectInstanceId;

#[derive(Clone)]
pub enum DesktopEvent {
    ModelsChanged,
    Resource(CommittedResourceMutation),
    Harness(HarnessEventEnvelope),
    Graph(
        ProjectInstanceId,
        yss_application::graph::editing::GraphActivity,
    ),
    Index(yss_project::ProjectIndexInvalidation),
    Ui(ProjectInstanceId, uuid::Uuid, yss_ui_contract::UiEvent),
}

#[derive(Clone)]
pub struct DesktopEvents {
    sender: broadcast::Sender<DesktopEvent>,
}

impl Default for DesktopEvents {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(512);
        Self { sender }
    }
}

impl DesktopEvents {
    pub fn subscribe(&self) -> broadcast::Receiver<DesktopEvent> {
        self.sender.subscribe()
    }

    pub fn publish(&self, event: DesktopEvent) {
        // Events are hints over authoritative snapshots; no subscribers is valid during startup.
        if self.sender.send(event).is_err() {
            tracing::trace!("Desktop event has no active subscribers");
        }
    }

    pub fn publish_resource(&self, mutation: CommittedResourceMutation) {
        self.publish(DesktopEvent::Resource(mutation));
    }

    pub fn models_changed(&self) {
        self.publish(DesktopEvent::ModelsChanged);
    }
}

pub struct ApplicationTasks {
    application: ApplicationState,
    executor: Handle,
    events: DesktopEvents,
}

impl ApplicationTasks {
    pub fn new(application: ApplicationState, executor: Handle, events: DesktopEvents) -> Self {
        Self {
            application,
            executor,
            events,
        }
    }

    pub fn application(&self) -> &ApplicationState {
        &self.application
    }

    pub fn executor(&self) -> &Handle {
        &self.executor
    }

    pub fn events(&self) -> &DesktopEvents {
        &self.events
    }

    pub fn publish_resource(&self, mutation: CommittedResourceMutation) {
        self.events.publish_resource(mutation);
    }

    /// Blocking use cases retain their owner until the real operation returns, even if a view closes.
    pub fn run<T: Send + 'static>(
        self: &Arc<Self>,
        operation: impl FnOnce(&ApplicationState) -> Result<T> + Send + 'static,
    ) -> JoinHandle<Result<T>> {
        let tasks = self.clone();
        self.executor
            .spawn_blocking(move || operation(&tasks.application))
    }
}
