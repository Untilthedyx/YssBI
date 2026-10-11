use gpui_kit::{App, Global};
use std::sync::Arc;
use yss_settings::UserSettings;

pub struct Preferences {
    settings: Arc<UserSettings>,
}
impl Global for Preferences {}

pub fn init(cx: &mut App) {
    cx.set_global(Preferences {
        settings: Arc::new(UserSettings::default()),
    });
}

pub fn current(cx: &App) -> &UserSettings {
    &cx.global::<Preferences>().settings
}

pub fn publish(settings: Arc<UserSettings>, cx: &mut App) -> anyhow::Result<()> {
    crate::keymap::validate_bindings(&settings.keybindings, cx)?;
    crate::text::set_locale(settings.language.locale());
    cx.set_global(Preferences { settings });
    crate::appearance::apply(cx);
    crate::keymap::apply_bindings(&current(cx).keybindings.clone(), cx)?;
    cx.refresh_windows();
    Ok(())
}
