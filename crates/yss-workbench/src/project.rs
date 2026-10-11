//! Native project and editor read models; mutation authority stays in Application/Project.
use yss_ui::resources;

use std::sync::Arc;

use yss_application::{
    activity_panel::ActivityPanelDocument, project::query::ProjectIndexSnapshot,
};
use yss_project_identity::ProjectInstanceId;

pub struct DesktopProject {
    pub identity: ProjectInstanceId,
    pub resources: Arc<resources::ResourceCatalog>,
    pub index: Arc<yss_project::ProjectIndex>,
    pub panels: Vec<Arc<ActivityPanelDocument>>,
    pub language: String,
}

impl DesktopProject {
    pub fn new(
        identity: ProjectInstanceId,
        snapshot: ProjectIndexSnapshot,
        language: &str,
    ) -> Self {
        let resources = Arc::new(resources::ResourceCatalog::new(
            identity.clone(),
            &snapshot.index,
        ));
        Self {
            resources,
            identity,
            language: language.to_owned(),
            index: Arc::new(snapshot.index),
            panels: snapshot.activity_panels.into_iter().map(Arc::new).collect(),
        }
    }
    pub fn first_graph(&self) -> Option<String> {
        self.index
            .event_graphs
            .first()
            .map(|graph| graph.path.clone())
            .or_else(|| {
                self.index
                    .function_graphs
                    .first()
                    .map(|graph| graph.path.clone())
            })
    }
}
