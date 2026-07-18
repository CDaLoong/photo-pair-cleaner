#![allow(dead_code)]

#[path = "../src/watermark_geometry.rs"]
mod watermark_geometry;
#[path = "../src/watermark_model.rs"]
mod watermark_model;

use watermark_geometry::{
    AnchorSpace, FrameEdge, PixelRect, ResolvedLayoutInput, anchor_region, clip_rect,
    normalized_placement, resolve_layout, resolve_preview_layout,
};
use watermark_model::{FrameInsets, NormalizedPlacement};

fn sample_layout() -> watermark_geometry::ResolvedLayout {
    resolve_layout(ResolvedLayoutInput {
        photo_width: 1200,
        photo_height: 800,
        output_long_edge: None,
        canvas_ratio: None,
        frame: FrameInsets {
            top: 0.05,
            right: 0.10,
            bottom: 0.20,
            left: 0.10,
        },
        align_x: 0.5,
        align_y: 0.5,
        photo_scale: 1.0,
    })
    .unwrap()
}

#[test]
fn asymmetric_frame_resolves_from_short_edge_ratios() {
    let layout = sample_layout();
    assert_eq!(layout.photo_rect.width, 1200);
    assert_eq!(layout.photo_rect.height, 800);
    assert_eq!(layout.photo_rect.x, 80);
    assert_eq!(layout.photo_rect.y, 40);
    assert_eq!(layout.canvas.width, 1360);
    assert_eq!(layout.canvas.height, 1000);
}

#[test]
fn fixed_ratio_expands_canvas_and_uses_photo_alignment() {
    let layout = resolve_layout(ResolvedLayoutInput {
        photo_width: 1200,
        photo_height: 800,
        output_long_edge: None,
        canvas_ratio: Some(1.0),
        frame: FrameInsets {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        },
        align_x: 0.5,
        align_y: 1.0,
        photo_scale: 1.0,
    })
    .unwrap();

    assert_eq!((layout.canvas.width, layout.canvas.height), (1200, 1200));
    assert_eq!((layout.photo_rect.x, layout.photo_rect.y), (0, 400));
}

#[test]
fn output_long_edge_scales_before_frame_geometry() {
    let layout = resolve_layout(ResolvedLayoutInput {
        photo_width: 6000,
        photo_height: 4000,
        output_long_edge: Some(1500),
        canvas_ratio: None,
        frame: FrameInsets {
            top: 0.1,
            right: 0.1,
            bottom: 0.1,
            left: 0.1,
        },
        align_x: 0.5,
        align_y: 0.5,
        photo_scale: 1.0,
    })
    .unwrap();

    assert_eq!(
        (layout.photo_rect.width, layout.photo_rect.height),
        (1500, 1000)
    );
    assert_eq!((layout.canvas.width, layout.canvas.height), (1700, 1200));
}

#[test]
fn bottom_frame_anchor_never_uses_the_photo_rect() {
    let layout = sample_layout();
    let region = anchor_region(&layout, AnchorSpace::Frame, Some(FrameEdge::Bottom)).unwrap();
    assert!(region.y >= layout.photo_rect.bottom());
    assert_eq!(region.bottom(), layout.canvas.height as i64);
}

#[test]
fn photo_and_canvas_anchor_spaces_remain_distinct() {
    let layout = sample_layout();
    assert_eq!(
        anchor_region(&layout, AnchorSpace::Photo, None).unwrap(),
        layout.photo_rect,
    );
    assert_eq!(
        anchor_region(&layout, AnchorSpace::Canvas, None).unwrap(),
        PixelRect {
            x: 0,
            y: 0,
            width: 1360,
            height: 1000
        },
    );
    assert!(anchor_region(&layout, AnchorSpace::Frame, None).is_err());
}

#[test]
fn normalized_layer_coordinates_resolve_against_the_anchor_region() {
    let region = PixelRect {
        x: 100,
        y: 200,
        width: 800,
        height: 100,
    };
    let placement = normalized_placement(
        region,
        &NormalizedPlacement {
            anchor_space: AnchorSpace::Frame,
            frame_edge: Some(FrameEdge::Bottom),
            x: 0.5,
            y: 0.5,
            width: 0.25,
            rotation_deg: 12.0,
            opacity: 0.8,
        },
    )
    .unwrap();

    assert_eq!((placement.center_x, placement.center_y), (500, 250));
    assert_eq!(placement.width, 200);
    assert_eq!(placement.rotation_deg, 12.0);
    assert_eq!(placement.opacity, 0.8);
}

#[test]
fn clipping_intersects_bleed_with_the_final_canvas() {
    let clipped = clip_rect(
        PixelRect {
            x: -20,
            y: 80,
            width: 80,
            height: 50,
        },
        watermark_geometry::PixelSize {
            width: 100,
            height: 100,
        },
    )
    .unwrap();
    assert_eq!(
        clipped,
        PixelRect {
            x: 0,
            y: 80,
            width: 60,
            height: 20
        }
    );
}

#[test]
fn preview_and_export_pixel_limits_are_enforced() {
    let large = ResolvedLayoutInput {
        photo_width: 5000,
        photo_height: 5000,
        output_long_edge: None,
        canvas_ratio: None,
        frame: FrameInsets {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        },
        align_x: 0.5,
        align_y: 0.5,
        photo_scale: 1.0,
    };
    assert!(
        resolve_preview_layout(large.clone())
            .unwrap_err()
            .contains("1600 万")
    );
    assert!(resolve_layout(large).is_ok());

    let too_large = ResolvedLayoutInput {
        photo_width: 15_000,
        photo_height: 15_000,
        output_long_edge: None,
        canvas_ratio: None,
        frame: FrameInsets {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        },
        align_x: 0.5,
        align_y: 0.5,
        photo_scale: 1.0,
    };
    assert!(resolve_layout(too_large).unwrap_err().contains("2 亿"));
}
