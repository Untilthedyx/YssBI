use super::{ActivityItem, ActivityPanel, ActivityRowContent, resources};
use gpui_kit::{Context, IntoElement};
use std::sync::Arc;
use yss_graph_ui::drag::GraphDrag;

pub(super) fn new(
    panel: &ActivityPanel,
    row: usize,
    cx: &Context<ActivityPanel>,
) -> Option<GraphDrag> {
    let document = panel.document.as_ref()?;
    let ActivityRowContent::Item(item) = &document.rows.get(row)?.content else {
        return None;
    };
    let creation = match item {
        ActivityItem::Node {
            available: true, ..
        }
        | ActivityItem::EventGraph { .. } => None,
        ActivityItem::FunctionGraph { path, .. } => Some(panel.resources.as_ref()?.creation(path)?),
        ActivityItem::Database { resource_path, .. } => {
            Some(panel.resources.as_ref()?.creation(resource_path)?)
        }
        _ => return None,
    };
    let source = cx.entity().downgrade();
    Some(GraphDrag::new(
        document.clone(),
        row,
        creation,
        move |document, creation, cx| {
            let Some(source) = source.upgrade() else {
                return false;
            };
            let source = source.read(cx);
            source.accepts(document)
                && creation.is_none_or(|catalog| {
                    source
                        .resources
                        .as_ref()
                        .is_some_and(|resources| Arc::ptr_eq(&resources.catalog, catalog))
                })
        },
        |item, cx| resources::label(item, cx).into_any_element(),
    ))
}
