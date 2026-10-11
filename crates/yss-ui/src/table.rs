//! Shared table presentation follows the current product density and stripe preferences.
use gpui_kit::component::{
    Sizable, Size,
    scroll::Scrollbar,
    table::{DataTable, TableDelegate, TableState},
};
use gpui_kit::{App, Entity, Pixels, px};

pub fn size(cx: &App) -> Size {
    match crate::preferences::current(cx).tables.density {
        yss_settings::Density::Compact => Size::Small,
        yss_settings::Density::Standard => Size::Medium,
        yss_settings::Density::Comfortable => Size::Large,
    }
}

pub fn present<D: TableDelegate>(state: &Entity<TableState<D>>, cx: &App) -> DataTable<D> {
    DataTable::new(state)
        .with_size(size(cx))
        .stripe(crate::preferences::current(cx).tables.stripe)
}

/// Match the component's header/row geometry and reserve its scrollbar plus outer hairlines.
pub fn height(visible_rows: usize, cx: &App) -> Pixels {
    size(cx).table_row_height() * (visible_rows as f32 + 1.) + Scrollbar::width() + px(2.)
}
