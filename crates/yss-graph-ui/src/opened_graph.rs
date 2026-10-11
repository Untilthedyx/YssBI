//! Native graph read model; mutation authority stays in Application/Project.
use anyhow::{Result, anyhow};
use std::sync::Arc;
use yss_application::graph::{
    editing::GraphEditResponse, open::OpenGraphApplicationReceipt, results::GraphResultState,
};
use yss_graph_editor::projection::EditorProjectionModel;
use yss_project::GraphEditingState;
use yss_project_identity::ProjectInstanceId;
pub struct OpenedGraph {
    pub(crate) project: ProjectInstanceId,
    pub(crate) projection: Arc<EditorProjectionModel>,
    pub(crate) editing: GraphEditingState,
    pub(crate) results: GraphResultState,
    pub(crate) language: String,
}

impl OpenedGraph {
    pub fn project(&self) -> &ProjectInstanceId {
        &self.project
    }
    pub fn projection(&self) -> &Arc<EditorProjectionModel> {
        &self.projection
    }
    pub fn editing(&self) -> &GraphEditingState {
        &self.editing
    }
    pub fn results(&self) -> &GraphResultState {
        &self.results
    }
    pub fn language(&self) -> &str {
        &self.language
    }

    pub fn from_open(receipt: OpenGraphApplicationReceipt, language: &str) -> Self {
        Self {
            project: receipt.project_instance_id().clone(),
            language: language.to_owned(),
            projection: Arc::new(receipt.projection().clone()),
            editing: receipt.editing().clone(),
            results: receipt.result_state().clone(),
        }
    }

    pub(crate) fn install_edit(
        &mut self,
        response: GraphEditResponse,
        language: &str,
    ) -> Result<()> {
        if response.update.projection_replacement.projection.graph_path
            != self.projection.graph_path
        {
            return Err(anyhow!(yss_ui::text::t("native.project.wrongResource")));
        }
        self.language = language.to_owned();
        self.projection = Arc::new(response.update.projection_replacement.projection);
        self.editing = response.editing;
        self.install_results(response.result_state);
        Ok(())
    }

    pub(crate) fn install_results(&mut self, state: GraphResultState) {
        if self.results.execution_session_id == state.execution_session_id
            && self.results.semantic_input_hash == state.semantic_input_hash
            && self.results.revision > state.revision
        {
            return;
        }
        self.results = state;
    }

    pub(crate) fn replace(&mut self, mut graph: Self) {
        if self.results.execution_session_id == graph.results.execution_session_id
            && self.results.semantic_input_hash == graph.results.semantic_input_hash
            && self.results.revision > graph.results.revision
        {
            graph.results = self.results.clone();
        }
        *self = graph;
    }
}
