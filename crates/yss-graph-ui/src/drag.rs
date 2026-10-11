//! Drag descriptors keep their original catalog identity; the source validates its own lifetime.
use gpui_kit::component::ActiveTheme;
use gpui_kit::{AnyElement, App, Context, IntoElement, Render, Window, div, prelude::*, px};
use std::{rc::Rc, sync::Arc};
use yss_application::activity_panel::{ActivityItem, ActivityPanelDocument, ActivityRowContent};
use yss_node_catalog::{NodeCreation, ResourceBoundCreateArgs};
use yss_project_identity::ProjectInstanceId;

type DragValidator =
    dyn Fn(&Arc<ActivityPanelDocument>, Option<&Arc<ActivityPanelDocument>>, &App) -> bool;

#[derive(Clone)]
pub struct GraphDrag {
    document: Arc<ActivityPanelDocument>,
    row: usize,
    creation: Option<(Arc<ActivityPanelDocument>, usize)>,
    validate: Rc<DragValidator>,
    render_resource: fn(&ActivityItem, &App) -> AnyElement,
}

pub enum GraphDrop<'a> {
    OpenGraph(&'a str),
    CreateNode(&'a NodeCreation),
}

impl GraphDrag {
    /// The validator must check the source entity, document identity and optional creation catalog identity.
    pub fn new(
        document: Arc<ActivityPanelDocument>,
        row: usize,
        creation: Option<(Arc<ActivityPanelDocument>, usize)>,
        validate: impl Fn(
            &Arc<ActivityPanelDocument>,
            Option<&Arc<ActivityPanelDocument>>,
            &App,
        ) -> bool
        + 'static,
        render_resource: fn(&ActivityItem, &App) -> AnyElement,
    ) -> Self {
        Self {
            document,
            row,
            creation,
            validate: Rc::new(validate),
            render_resource,
        }
    }

    pub fn resolve(
        &self,
        project: &ProjectInstanceId,
        target_path: &str,
        cx: &App,
    ) -> Option<GraphDrop<'_>> {
        if self.document.project_instance_id.as_deref() != Some(project.as_str())
            || !(self.validate)(
                &self.document,
                self.creation.as_ref().map(|(catalog, _)| catalog),
                cx,
            )
        {
            return None;
        }
        let ActivityRowContent::Item(item) = &self.document.rows.get(self.row)?.content else {
            return None;
        };
        if let ActivityItem::EventGraph { path, .. } = item {
            return Some(GraphDrop::OpenGraph(path));
        }
        let (document, row) = self
            .creation
            .as_ref()
            .map_or((&self.document, self.row), |(catalog, row)| (catalog, *row));
        let ActivityRowContent::Item(ActivityItem::Node {
            available: true,
            creation,
            ..
        }) = &document.rows.get(row)?.content
        else {
            return None;
        };
        if let NodeCreation::ResourceBound {
            resource_path,
            create_args: ResourceBoundCreateArgs::FunctionGraph,
            ..
        } = creation
            && resource_path.as_str() == target_path
        {
            return match item {
                ActivityItem::FunctionGraph { path, .. } => Some(GraphDrop::OpenGraph(path)),
                _ => None,
            };
        }
        Some(GraphDrop::CreateNode(creation))
    }
}

impl Render for GraphDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = div()
            .p_2()
            .max_w(px(280.))
            .rounded_md()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .border_1()
            .border_color(cx.theme().border);
        match self.document.rows.get(self.row).map(|row| &row.content) {
            Some(ActivityRowContent::Item(ActivityItem::Node {
                title,
                creation,
                available,
                ..
            })) => body.child(crate::catalog_rows::node(title, creation, *available, cx)),
            Some(ActivityRowContent::Item(item)) => body.child((self.render_resource)(item, cx)),
            _ => body,
        }
    }
}
