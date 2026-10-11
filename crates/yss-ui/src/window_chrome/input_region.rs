use anyhow::Result;
use gpui_kit::{Bounds, Pixels, Window};
use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle};
use x11rb::{
    protocol::{
        shape::{ConnectionExt as _, SK, SO},
        xproto::{ClipOrdering, Rectangle},
    },
    rust_connection::RustConnection,
};

#[derive(Default)]
pub(super) struct WindowInputRegion {
    applied: Option<(Option<Bounds<Pixels>>, f32)>,
    x11_connection: Option<RustConnection>,
}

impl WindowInputRegion {
    pub(super) fn update(&mut self, window: &Window, bounds: Option<Bounds<Pixels>>) -> Result<()> {
        let scale_factor = window.scale_factor();
        if self.applied == Some((bounds, scale_factor)) {
            return Ok(());
        }
        let handle = match HasWindowHandle::window_handle(window) {
            Ok(handle) => handle,
            Err(HandleError::NotSupported) => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        match handle.as_raw() {
            RawWindowHandle::Wayland(_) => {
                window.set_input_region(bounds.as_ref().map(std::slice::from_ref));
            }
            RawWindowHandle::Xcb(handle) => {
                // GPUI's set_input_region only implements Wayland. A separate owned
                // connection avoids retaining or taking ownership of GPUI's raw XCB pointer.
                if self.x11_connection.is_none() {
                    self.x11_connection = Some(x11rb::connect(None)?.0);
                }
                if let Some(connection) = &self.x11_connection {
                    if let Some(bounds) = bounds {
                        connection
                            .shape_rectangles(
                                SO::SET,
                                SK::INPUT,
                                ClipOrdering::UNSORTED,
                                handle.window.get(),
                                0,
                                0,
                                &[physical_rectangle(bounds, scale_factor)?],
                            )?
                            .check()?;
                    } else {
                        connection
                            .shape_mask(SO::SET, SK::INPUT, handle.window.get(), 0, 0, x11rb::NONE)?
                            .check()?;
                    }
                }
            }
            _ => return Ok(()),
        }
        self.applied = Some((bounds, scale_factor));
        Ok(())
    }
}

fn physical_rectangle(bounds: Bounds<Pixels>, scale_factor: f32) -> Result<Rectangle> {
    // Round outwards so fractional display scaling cannot cut into the resize band.
    let left = (f32::from(bounds.left()) * scale_factor).floor() as i32;
    let top = (f32::from(bounds.top()) * scale_factor).floor() as i32;
    let right = (f32::from(bounds.right()) * scale_factor).ceil() as i32;
    let bottom = (f32::from(bounds.bottom()) * scale_factor).ceil() as i32;
    Ok(Rectangle {
        x: left.try_into()?,
        y: top.try_into()?,
        width: (right - left).try_into()?,
        height: (bottom - top).try_into()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{point, px, size};

    #[test]
    fn fractional_scale_keeps_the_entire_input_band() -> Result<()> {
        let rectangle = physical_rectangle(
            Bounds::new(point(px(15.), px(16.)), size(px(971.), px(668.))),
            1.25,
        )?;
        assert_eq!(
            (rectangle.x, rectangle.y, rectangle.width, rectangle.height),
            (18, 20, 1215, 835),
        );
        Ok(())
    }
}
