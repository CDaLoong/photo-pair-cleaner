use objc2::{
    msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject},
};
use objc2_app_kit::NSView;
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString, NSURL};
use std::cell::RefCell;
use std::ffi::c_void;
use std::path::Path;

#[link(name = "QuickLookUI", kind = "framework")]
unsafe extern "C" {}

#[derive(Clone, Copy)]
pub(crate) struct PreviewRect {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) viewport_width: f64,
    pub(crate) viewport_height: f64,
}

struct PreviewView {
    id: String,
    source: String,
    container: Retained<NSView>,
    view: Retained<NSView>,
}

thread_local! {
    static PREVIEW_VIEW: RefCell<Option<PreviewView>> = const { RefCell::new(None) };
}

fn scaled_frame(bounds: NSRect, flipped: bool, rect: PreviewRect) -> Result<NSRect, String> {
    if !rect.x.is_finite()
        || !rect.y.is_finite()
        || !rect.width.is_finite()
        || !rect.height.is_finite()
        || !rect.viewport_width.is_finite()
        || !rect.viewport_height.is_finite()
        || rect.width <= 0.0
        || rect.height <= 0.0
        || rect.viewport_width <= 0.0
        || rect.viewport_height <= 0.0
    {
        return Err("原生预览区域无效".to_string());
    }

    let scale_x = bounds.size.width / rect.viewport_width;
    let scale_y = bounds.size.height / rect.viewport_height;
    let x = rect.x * scale_x;
    let top = rect.y * scale_y;
    let width = rect.width * scale_x;
    let height = rect.height * scale_y;
    let y = if flipped {
        top
    } else {
        bounds.size.height - top - height
    };
    Ok(NSRect::new(NSPoint::new(x, y), NSSize::new(width, height)))
}

fn native_frame(parent: &NSView, rect: PreviewRect) -> Result<NSRect, String> {
    scaled_frame(parent.bounds(), parent.isFlipped(), rect)
}

unsafe fn set_preview_item(view: &NSView, source: &str) {
    let path = NSString::from_str(source);
    let url = NSURL::fileURLWithPath(&path);
    unsafe {
        let _: () = msg_send![view, setPreviewItem: &*url];
        let _: () = msg_send![view, setAutostarts: true];
        let _: () = msg_send![view, setShouldCloseWithWindow: true];
    }
}

unsafe fn create_clipping_container(frame: NSRect) -> Result<Retained<NSView>, String> {
    let class = AnyClass::get(c"NSView").ok_or_else(|| "无法创建原生预览容器".to_string())?;
    let allocated: *mut NSView = unsafe { msg_send![class, alloc] };
    let initialized: *mut NSView = unsafe { msg_send![allocated, initWithFrame: frame] };
    let container = unsafe { Retained::from_raw(initialized) }
        .ok_or_else(|| "无法初始化原生预览容器".to_string())?;
    unsafe {
        let _: () = msg_send![&*container, setWantsLayer: true];
        let layer: *mut AnyObject = msg_send![&*container, layer];
        if !layer.is_null() {
            let _: () = msg_send![layer, setMasksToBounds: true];
        }
    }
    Ok(container)
}

fn layout_preview(preview: &PreviewView, frame: NSRect) {
    preview.container.setFrame(frame);
    preview.view.setFrame(preview.container.bounds());
}

pub(crate) unsafe fn show(
    webview: *mut c_void,
    preview_id: String,
    source: &Path,
    rect: PreviewRect,
) -> Result<(), String> {
    let parent = unsafe { &*webview.cast::<NSView>() };
    let frame = native_frame(parent, rect)?;
    let source = source.to_string_lossy().into_owned();

    PREVIEW_VIEW.with(|slot| {
        let mut slot = slot.borrow_mut();
        if let Some(preview) = slot.as_mut() {
            layout_preview(preview, frame);
            if preview.source != source {
                unsafe { set_preview_item(&preview.view, &source) };
                preview.source.clone_from(&source);
            }
            preview.id = preview_id;
            return Ok(());
        }

        let class = AnyClass::get(c"QLPreviewView")
            .ok_or_else(|| "当前 macOS 不支持 Quick Look 原生预览".to_string())?;
        let allocated: *mut NSView = unsafe { msg_send![class, alloc] };
        let initialized: *mut NSView =
            unsafe { msg_send![allocated, initWithFrame: NSRect::new(NSPoint::new(0.0, 0.0), frame.size), style: 0usize] };
        let view = unsafe { Retained::from_raw(initialized) }
            .ok_or_else(|| "无法创建 Quick Look 原生预览".to_string())?;
        let container = unsafe { create_clipping_container(frame)? };
        unsafe {
            let _: () = msg_send![&*view, setAutoresizingMask: 18usize];
        }
        unsafe { set_preview_item(&view, &source) };
        container.addSubview(&view);
        parent.addSubview(&container);
        *slot = Some(PreviewView {
            id: preview_id,
            source,
            container,
            view,
        });
        Ok(())
    })
}

pub(crate) unsafe fn hide(preview_id: &str) {
    PREVIEW_VIEW.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot
            .as_ref()
            .is_some_and(|preview| preview.id == preview_id)
            && let Some(preview) = slot.take()
        {
            unsafe {
                let _: () = msg_send![&*preview.view, close];
            }
            preview.container.removeFromSuperview();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_rect_scales_css_coordinates_for_flipped_and_unflipped_views() {
        let bounds = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1180.0, 780.0));
        let rect = PreviewRect {
            x: 420.0,
            y: 200.0,
            width: 600.0,
            height: 400.0,
            viewport_width: 1180.0,
            viewport_height: 780.0,
        };

        let flipped = scaled_frame(bounds, true, rect).expect("flipped frame");
        let unflipped = scaled_frame(bounds, false, rect).expect("unflipped frame");

        assert_eq!(flipped.origin, NSPoint::new(420.0, 200.0));
        assert_eq!(unflipped.origin, NSPoint::new(420.0, 180.0));
        assert_eq!(flipped.size, NSSize::new(600.0, 400.0));
    }
}
