use super::*;
use crate::{GraphServices, OpenedGraph};
use gpui_kit::{AppContext, MouseButton, MouseDownEvent, TestAppContext, WindowOptions, point, px};
use std::sync::Arc;
use yss_application::{
    graph::{editing::GraphEditRequest, open::OpenGraphRequest},
    session::ApplicationState,
};
use yss_graph_document::{DocumentNode, GraphResourceKind, NodeId, NodePosition};
use yss_graph_editor::EditorGraphMutation;
use yss_project_history::{
    DatabaseResourceKey, DocResourceKey, FunctionResourceKey, MindResourceKey, ResourceKey,
};
use yss_project_identity::OperationId;

struct Fixture {
    directory: tempfile::TempDir,
    application: ApplicationState,
    request: OpenGraphRequest,
    source: NodeId,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = GraphResourcePath::new("events/Refresh.yssbi-event").unwrap();
        let mut data = yss_project_model::ProjectData::new();
        let mut resource =
            yss_project_model::GraphResourceDocument::new("Refresh", GraphResourceKind::EventGraph);
        let source = NodeId::new();
        for (id, kind, parameter, value) in [
            (
                source,
                "yssbi.dataframe.source.get",
                "dataframe",
                "missing-data",
            ),
            (
                NodeId::new(),
                "yssbi.project.function.call",
                "target",
                "functions/Missing.yssbi-function",
            ),
        ] {
            Arc::make_mut(&mut resource.document).nodes.insert(
                id,
                DocumentNode {
                    id,
                    node_type: kind.parse().unwrap(),
                    position: NodePosition { x: 0.0, y: 0.0 },
                    parameters: [(parameter.parse().unwrap(), serde_json::json!(value))].into(),
                    user_label: None,
                },
            );
        }
        data.graphs.insert(path.clone(), resource);
        yss_project::fixtures::write_project(&data, directory.path().to_str().unwrap()).unwrap();
        let application = ApplicationState::initialize().unwrap();
        application
            .load_project_for_application(directory.path().join("metadata.yssbi").to_str().unwrap())
            .unwrap();
        let project = application
            .capture_session()
            .unwrap()
            .project_instance_id()
            .clone();
        let request = OpenGraphRequest::new(project, path, 0, yss_ui::text::locale());
        Self {
            directory,
            application,
            request,
            source,
        }
    }

    fn open(&self) -> OpenedGraph {
        OpenedGraph::from_open(
            self.application.open_graph(self.request.clone()).unwrap(),
            self.request.locale(),
        )
    }

    fn edit(
        &self,
        graph: &OpenedGraph,
        mutation: EditorGraphMutation,
    ) -> yss_application::graph::editing::GraphEditResponse {
        self.application
            .edit_graph(
                GraphEditRequest {
                    project_instance_id: graph.project().clone(),
                    graph_path: graph.projection().graph_path.clone(),
                    version: graph.editing().version,
                    operation_id: OperationId::new(),
                    locale: graph.language().to_owned(),
                },
                mutation,
            )
            .unwrap()
    }
}

#[test]
fn routing_uses_observed_missing_dependencies_and_tracks_edits() {
    let fixture = Fixture::new();
    let mut graph = fixture.open();
    let database = |id: &str| ResourceKey::Database(DatabaseResourceKey(id.into()));
    assert!(graph.depends_on_resource(&database("missing-data")));
    assert!(!graph.depends_on_resource(&database("unrelated")));
    assert!(
        graph.depends_on_resource(&ResourceKey::Function(FunctionResourceKey(
            "functions/Missing.yssbi-function".into()
        )))
    );
    assert!(!graph.depends_on_resource(&ResourceKey::Doc(DocResourceKey("docs/Notes.md".into()))));
    assert!(
        !graph.depends_on_resource(&ResourceKey::Mind(MindResourceKey(
            "minds/Notes.yssbi-mind".into()
        )))
    );
    let edited = fixture.edit(
        &graph,
        EditorGraphMutation::DeleteNodes {
            node_ids: vec![fixture.source],
        },
    );
    graph
        .install_edit(edited, fixture.request.locale())
        .unwrap();
    assert!(!graph.depends_on_resource(&database("missing-data")));
    assert!(
        graph.depends_on_resource(&ResourceKey::Function(FunctionResourceKey(
            "functions/Missing.yssbi-function".into()
        )))
    );
}

#[gpui_kit::test]
fn unchanged_refresh_preserves_gesture_and_projection_but_real_edit_replaces_them(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let tasks = Arc::new(yss_desktop_services::ApplicationTasks::new(
        fixture.application.clone(),
        runtime.handle().clone(),
        Default::default(),
    ));
    let services = Arc::new(GraphServices::new(
        tasks,
        Arc::new(yss_ui::layout_store::LayoutStore::new(
            fixture.directory.path(),
        )),
    ));
    let graph = fixture.open();
    let same = fixture.open();
    let catalog = Arc::new(
        fixture
            .application
            .query_project_index(
                fixture.request.project_instance_id().clone(),
                fixture.request.locale(),
                true,
            )
            .unwrap()
            .activity_panels
            .into_iter()
            .find(|panel| panel.panel_id == "nodes")
            .unwrap(),
    );
    cx.update(|cx| {
        gpui_kit::init(cx);
        yss_ui::preferences::init(cx);
    });
    let (window, canvas) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                GraphCanvas::new(
                    services,
                    graph,
                    catalog,
                    yss_ui::text::locale().into(),
                    window,
                    cx,
                )
            })
        })
        .unwrap()
    });
    cx.update_window(window, |_, window, cx| {
        canvas.update(cx, |canvas, cx| {
            canvas.begin_pan(
                None,
                &MouseDownEvent {
                    button: MouseButton::Middle,
                    position: point(px(40.), px(40.)),
                    ..Default::default()
                },
                window,
                cx,
            );
            let projection = canvas.graph.projection.clone();
            let presentation = canvas.presentation.clone();
            let details = canvas.port_details.clone();
            canvas.install_projection(same, cx);
            assert!(
                canvas.gesture.is_some(),
                "an equal read cancelled the active pan"
            );
            assert!(Arc::ptr_eq(&projection, &canvas.graph.projection));
            assert!(std::rc::Rc::ptr_eq(&presentation, &canvas.presentation));
            assert!(
                details
                    .as_ref()
                    .zip(canvas.port_details.as_ref())
                    .is_some_and(|(before, after)| std::rc::Rc::ptr_eq(before, after))
            );
            let edited = fixture.edit(
                &canvas.graph,
                EditorGraphMutation::SetNodeLabel {
                    node_id: fixture.source,
                    label: Some("Changed source".into()),
                },
            );
            let changed = fixture.open();
            assert_eq!(changed.editing().version, edited.editing.version);
            canvas.install_projection(changed, cx);
            assert!(canvas.gesture.is_none());
            assert!(!Arc::ptr_eq(&projection, &canvas.graph.projection));
            assert!(
                canvas
                    .graph
                    .projection
                    .nodes
                    .iter()
                    .any(|node| node.display.user_label.as_deref() == Some("Changed source"))
            );
        });
    })
    .unwrap();
}

#[test]
fn refresh_identity_includes_locale_runtime_and_resolver_basis() {
    let fixture = Fixture::new();
    let original = fixture.open();
    let mut incoming = fixture.open();
    assert!(original.same_projection(&incoming));
    incoming.language = "another-language".into();
    assert!(!original.same_projection(&incoming));
    incoming.language = original.language.clone();
    incoming.results.execution_session_id =
        yss_graph_execution::identity::ExecutionSessionId::new(uuid::Uuid::new_v4());
    assert!(!original.same_projection(&incoming));
    incoming.results.execution_session_id = original.results.execution_session_id;
    Arc::make_mut(&mut incoming.projection)
        .basis
        .registry_fingerprint = [42; 32];
    assert!(!original.same_projection(&incoming));
}
