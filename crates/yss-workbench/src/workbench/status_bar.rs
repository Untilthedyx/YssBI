//! Each visible Dock column owns its shortcuts and graph statistics.
use super::{Workbench, layout::columns, menus::WorkbenchPanel};
use crate::canvas::GraphCanvas;
use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::{
    Disableable, Icon, Selectable, Sizable,
    button::{Button, ButtonVariants},
    dock::DockPlacement,
    status_bar::StatusBar,
    tooltip::Tooltip,
};
use gpui_kit::{
    App, Context, Entity, IntoElement, Pixels, RenderOnce, WeakEntity, Window, div, prelude::*, px,
};

pub(super) const STATUS_BAR_HEIGHT: Pixels = px(28.);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum StatusColumn {
    Left,
    Conversation,
    Editor,
    Right,
}

#[derive(IntoElement)]
pub(super) struct ColumnStatusBar {
    owner: WeakEntity<Workbench>,
    column: StatusColumn,
}

impl ColumnStatusBar {
    pub(super) fn new(owner: WeakEntity<Workbench>, column: StatusColumn) -> Self {
        Self { owner, column }
    }
}

impl RenderOnce for ColumnStatusBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        // Dock's renderer only constructs this component. Read Dock after its
        // render borrow has ended, when GPUI renders the child.
        let bar = self
            .owner
            .upgrade()
            .map(|owner| owner.update(cx, |owner, cx| owner.column_status_bar(self.column, cx)))
            .unwrap_or_else(StatusBar::new);
        let id = match self.column {
            StatusColumn::Left => "statusbar-left",
            StatusColumn::Conversation => "statusbar-conversation",
            StatusColumn::Editor => "statusbar-editor",
            StatusColumn::Right => "statusbar-right",
        };
        div()
            .id(id)
            .w_full()
            .min_w_0()
            .h(STATUS_BAR_HEIGHT)
            .flex_shrink_0()
            .child(bar.w_full().h_full().py_0().gap_0())
            .test_support()
    }
}

impl Workbench {
    pub(super) fn observe_graph_status(
        &mut self,
        canvas: &Entity<GraphCanvas>,
        cx: &mut Context<Self>,
    ) {
        let mut status = GraphStatus::read(canvas.read(cx));
        self.subscriptions
            .push(cx.observe(canvas, move |_, canvas, cx| {
                let next = GraphStatus::read(canvas.read(cx));
                // Only changes to the displayed values redraw the workbench chrome.
                if next != status {
                    status = next;
                    cx.notify();
                }
            }));
    }

    fn column_status_bar(&self, column: StatusColumn, cx: &mut Context<Self>) -> StatusBar {
        let dock = self.dock.read(cx);
        let has_project = self.project.is_some();
        let conversation_zoomed = columns::conversation_zoomed(dock);
        let left_visible = has_project
            && dock.is_dock_open(DockPlacement::Left)
            && (!dock.is_zoomed() || conversation_zoomed);
        let conversation_visible =
            has_project && (conversation_zoomed || columns::conversation_edge(dock).is_some());
        let right_visible =
            has_project && dock.is_dock_open(DockPlacement::Right) && !dock.is_zoomed();

        let bar = StatusBar::new();
        match column {
            StatusColumn::Left => bar.left(
                div()
                    .flex()
                    .items_center()
                    .child(self.left_status_button(cx))
                    .when(!conversation_visible, |row| {
                        row.child(self.conversation_window_button(cx))
                    }),
            ),
            StatusColumn::Conversation => bar
                .left(
                    div()
                        .flex()
                        .items_center()
                        .when(!left_visible, |row| row.child(self.left_status_button(cx)))
                        .child(self.conversation_window_button(cx)),
                )
                .when(conversation_zoomed && !right_visible, |bar| {
                    bar.right(self.right_status_button(cx))
                }),
            StatusColumn::Editor => bar
                .left(
                    div()
                        .flex()
                        .items_center()
                        .when(!left_visible && !conversation_visible, |row| {
                            row.child(self.left_status_button(cx))
                                .child(self.conversation_window_button(cx))
                        })
                        .child(self.panel_button(
                            "status-problems",
                            IconName::TriangleAlert,
                            crate::text::t("detail.sections.diagnostics"),
                            WorkbenchPanel::Problems,
                            cx,
                        ))
                        .child(self.panel_button(
                            "status-output",
                            IconName::SquareTerminal,
                            crate::text::t("panel.output"),
                            WorkbenchPanel::Output,
                            cx,
                        ))
                        .child(self.panel_button(
                            "status-results",
                            IconName::Table,
                            crate::text::t("panel.results"),
                            WorkbenchPanel::Results,
                            cx,
                        ))
                        .child(self.panel_button(
                            "status-logs",
                            IconName::FileText,
                            crate::text::t("log.title"),
                            WorkbenchPanel::Logs,
                            cx,
                        )),
                )
                .right(
                    div()
                        .flex()
                        .items_center()
                        .when(!right_visible, |row| {
                            row.child(self.right_status_button(cx))
                        })
                        .when_some(self.graph_status(cx), |row, status| {
                            row.child(status.render())
                        }),
                ),
            StatusColumn::Right => bar.right(self.right_status_button(cx)),
        }
    }

    fn graph_status(&self, cx: &Context<Self>) -> Option<GraphStatus> {
        let dock = self.dock.read(cx);
        self.project
            .as_ref()
            .and_then(|_| self.active_editor_panel(cx))
            .and_then(|panel| {
                let id = panel.panel_id(cx);
                let placement = self.displayed_panel_placement(id, cx)?;
                if let Some(zoomed) = dock.zoomed_group()
                    && dock.layout(placement)?.find_panel_node(id) != Some(zoomed)
                {
                    return None;
                }
                let canvas = panel.view().downcast::<GraphCanvas>().ok()?;
                Some(GraphStatus::read(canvas.read(cx)))
            })
    }

    fn left_status_button(&self, cx: &mut Context<Self>) -> Button {
        self.dock_button(
            "status-left",
            IconName::PanelLeft,
            crate::text::t("native.workbench.toggleLeftSidebar"),
            DockPlacement::Left,
            cx,
        )
    }

    fn right_status_button(&self, cx: &mut Context<Self>) -> Button {
        self.dock_button(
            "status-right",
            IconName::PanelRight,
            crate::text::t("native.workbench.toggleRightSidebar"),
            DockPlacement::Right,
            cx,
        )
    }

    fn conversation_window_button(&self, cx: &mut Context<Self>) -> Button {
        let open = self.project.is_some() && self.conversation_window_open(cx);
        let label = if open {
            "bottomBar.closeConversation"
        } else {
            "bottomBar.openConversation"
        };
        Button::new("status-assistant")
            .xsmall()
            .compact()
            .ghost()
            .icon(Icon::new(gpui_kit::assets::IconName::MessageSquareText).size_3())
            .tooltip(crate::text::translate(label))
            .selected(open)
            .disabled(
                self.project.is_none()
                    || self.assistant.read(cx).is_reopening()
                    || self.is_closing(cx)
                    || self.dock.read(cx).is_locked(),
            )
            .on_click(cx.listener(|view, _, window, cx| {
                view.toggle_conversation_window(window, cx);
            }))
    }

    fn panel_button(
        &self,
        id: &'static str,
        icon: IconName,
        tooltip: std::borrow::Cow<'static, str>,
        panel: WorkbenchPanel,
        cx: &mut Context<Self>,
    ) -> Button {
        Button::new(id)
            .xsmall()
            .compact()
            .ghost()
            .icon(Icon::new(icon).size_3())
            .tooltip(tooltip)
            .selected(
                self.project.is_some()
                    && !self.dock.read(cx).is_zoomed()
                    && self.panel_is_displayed(panel, cx),
            )
            .disabled(self.project.is_none() || self.is_closing(cx))
            .on_click(cx.listener(move |view, _, window, cx| {
                view.toggle_panel(panel, window, cx);
            }))
    }

    fn dock_button(
        &self,
        id: &'static str,
        icon: IconName,
        tooltip: std::borrow::Cow<'static, str>,
        placement: DockPlacement,
        cx: &mut Context<Self>,
    ) -> Button {
        let dock = self.dock.read(cx);
        Button::new(id)
            .xsmall()
            .compact()
            .ghost()
            .icon(Icon::new(icon).size_3())
            .tooltip(tooltip)
            .selected(
                self.project.is_some()
                    && dock.is_dock_open(placement)
                    && (!dock.is_zoomed()
                        || placement == DockPlacement::Left && columns::conversation_zoomed(dock)),
            )
            .disabled(self.project.is_none() || !dock.has_dock(placement) || self.is_closing(cx))
            .on_click(cx.listener(move |view, _, window, cx| {
                view.toggle_dock_region(placement, window, cx);
            }))
    }

    pub(super) fn toggle_dock_region(
        &self,
        placement: DockPlacement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_closing(cx) || self.project.is_none() {
            return;
        }
        self.dock.update(cx, |dock, cx| {
            if dock.is_zoomed()
                && !(placement == DockPlacement::Left && columns::conversation_zoomed(dock))
            {
                dock.set_zoomed_out(window, cx);
                if !dock.is_dock_open(placement) {
                    dock.toggle_dock(placement, window, cx);
                }
            } else {
                dock.toggle_dock(placement, window, cx);
            }
        });
    }
}

#[derive(Clone, PartialEq, Eq)]
struct GraphStatus {
    nodes: usize,
    connections: usize,
    offset_x: i32,
    offset_y: i32,
    zoom_percent: u16,
    busy: bool,
    dirty: bool,
    run_status: std::borrow::Cow<'static, str>,
}

impl GraphStatus {
    fn read(canvas: &GraphCanvas) -> Self {
        let offset = canvas.viewport_offset();
        Self {
            nodes: canvas.graph().projection().nodes.len(),
            connections: canvas.graph().projection().connections.len(),
            offset_x: f32::from(offset.x).round() as i32,
            offset_y: f32::from(offset.y).round() as i32,
            zoom_percent: (canvas.zoom() * 100.).round() as u16,
            busy: canvas.busy(),
            dirty: canvas.dirty(),
            run_status: canvas.run_status(),
        }
    }

    fn render(self) -> impl IntoElement {
        div()
            .h_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_2()
            .when(self.busy, |view| {
                view.child(status_item(
                    "status-graph-busy",
                    IconName::LoaderCircle,
                    String::new(),
                    crate::text::t("native.databases.submitting").into(),
                ))
            })
            .when(!self.run_status.is_empty(), |view| {
                view.child(status_item(
                    "status-graph-run",
                    IconName::Activity,
                    String::new(),
                    self.run_status.into(),
                ))
            })
            .child(status_item(
                "status-graph-nodes",
                IconName::Workflow,
                self.nodes.to_string(),
                crate::text::format(
                    "native.workbench.nodeCount",
                    &[("value0", self.nodes.to_string())],
                ),
            ))
            .child(status_item(
                "status-graph-connections",
                IconName::Cable,
                self.connections.to_string(),
                crate::text::format(
                    "native.workbench.edgeCount",
                    &[("value0", self.connections.to_string())],
                ),
            ))
            .child(status_item(
                "status-graph-position",
                IconName::Move,
                format!("X {} Y {}", self.offset_x, self.offset_y),
                crate::text::t("native.workbench.viewportPosition").into(),
            ))
            .child(status_item(
                "status-graph-zoom",
                IconName::ZoomIn,
                format!("{}%", self.zoom_percent),
                crate::text::t("native.workbench.viewportZoom").into(),
            ))
    }
}

fn status_item(
    id: &'static str,
    icon: IconName,
    label: String,
    tooltip: String,
) -> impl IntoElement {
    div()
        .id(id)
        .h_full()
        .px_1()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap_1()
        .whitespace_nowrap()
        .child(Icon::new(icon).size_3())
        .when(!label.is_empty(), |view| view.child(label))
        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
}
