//! Original cursor artwork for the note editor, with the drawing tip as its hotspot.

use winit::window::{CustomCursor, CustomCursorSource};

pub fn pencil(scale: f64) -> CustomCursorSource {
    let scale = scale.clamp(1.0, 4.0) as f32;
    let size = (32.0 * scale).round() as u16;
    let mut image = tiny_skia::Pixmap::new(u32::from(size), u32::from(size)).unwrap();
    let transform = tiny_skia::Transform::from_scale(scale, scale);
    let mut path = tiny_skia::PathBuilder::new();
    path.move_to(1.0, 1.0);
    path.line_to(5.0, 1.0);
    path.line_to(15.0, 11.0);
    path.line_to(15.0, 13.0);
    path.line_to(13.0, 15.0);
    path.line_to(11.0, 15.0);
    path.line_to(1.0, 5.0);
    path.close();
    let path = path.finish().unwrap();
    let mut ink = tiny_skia::Paint::default();
    ink.set_color_rgba8(255, 255, 255, 255);
    image.fill_path(&path, &ink, tiny_skia::FillRule::Winding, transform, None);
    image.stroke_path(&path, &ink, &tiny_skia::Stroke { width: 2.5, ..Default::default() }, transform, None);
    ink.set_color_rgba8(20, 20, 20, 255);
    image.stroke_path(&path, &ink, &tiny_skia::Stroke { width: 1.0, ..Default::default() }, transform, None);
    let mut line = tiny_skia::PathBuilder::new();
    line.move_to(2.0, 5.0);
    line.line_to(5.0, 2.0);
    line.move_to(4.5, 4.5);
    line.line_to(13.0, 13.0);
    image.stroke_path(&line.finish().unwrap(), &ink, &tiny_skia::Stroke { width: 1.0, ..Default::default() }, transform, None);
    // winit expects straight alpha, while tiny-skia renders premultiplied alpha.
    let rgba: Vec<u8> = image.pixels().iter().flat_map(|p| {
        let p = p.demultiply();
        [p.red(), p.green(), p.blue(), p.alpha()]
    }).collect();
    let tip = scale.round() as u16;
    CustomCursor::from_rgba(rgba, size, size, tip, tip).unwrap()
}
