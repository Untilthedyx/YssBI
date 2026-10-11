//! Conversation visibility comes from DockArea; the closed ID is only a reopening hint.
use super::{ConversationPanel, Workbench};
use gpui_kit::component::dock::DockPlacement;
use gpui_kit::{App, Context, Entity, Focusable, Window};

impl Workbench {
    fn docked_conversations<'a>(
        &'a self,
        cx: &'a App,
    ) -> impl Iterator<Item = Entity<ConversationPanel>> + 'a {
        let dock = self.dock.read(cx);
        [
            DockPlacement::Center,
            DockPlacement::Left,
            DockPlacement::Right,
            DockPlacement::Bottom,
        ]
        .into_iter()
        .filter_map(move |placement| dock.layout(placement))
        .flat_map(|tree| tree.panels())
        .filter_map(|id| dock.panel(id)?.view().downcast::<ConversationPanel>().ok())
    }

    pub(in crate::workbench) fn conversation_window_open(&self, cx: &App) -> bool {
        self.docked_conversations(cx).next().is_some()
    }

    pub(in crate::workbench) fn toggle_conversation_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.project.is_none()
            || self.is_closing(cx)
            || self.assistant.read(cx).is_reopening()
            || self.dock.read(cx).is_locked()
        {
            return;
        }
        if self.conversation_window_open(cx) {
            self.close_conversation_window(window, cx);
        } else {
            self.assistant
                .update(cx, |assistant, cx| assistant.request_reopen(window, cx));
        }
    }

    pub(in crate::workbench) fn close_conversation_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_closing(cx) || self.dock.read(cx).is_locked() {
            return;
        }
        let panels: Vec<_> = self.docked_conversations(cx).collect();
        if panels.is_empty() {
            return;
        }
        let selected = self
            .visible_conversation(cx)
            .or_else(|| panels.first().cloned());
        let closed = selected.map(|panel| panel.read(cx).session_id().to_string());
        self.assistant
            .update(cx, |assistant, cx| assistant.note_closed(closed, cx));
        self.dock.update(cx, |dock, cx| {
            dock.set_zoomed_out(window, cx);
            for panel in panels {
                dock.remove_panel(panel, window, cx);
            }
            dock.focus_handle(cx).focus(window, cx);
        });
        self.sync_active_conversation(cx);
        cx.notify();
    }
}
