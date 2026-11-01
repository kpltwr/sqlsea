//! The close mark, drawn as two lines. A font glyph such as "×" sits low in
//! its line and has a different size in each font, so it does not center.

use iced::widget::canvas::{self, LineCap, Path, Stroke};
use iced::{Color, Element, Point, Rectangle, Renderer, Theme, mouse};

/// A cross of `size` pixels, centered in its bounds.
pub fn cross<'a, M: 'a>(size: f32, color: fn(&Theme) -> Color) -> Element<'a, M> {
    canvas::Canvas::new(Cross { color })
        .width(size)
        .height(size)
        .into()
}

struct Cross {
    color: fn(&Theme) -> Color,
}

impl<M> canvas::Program<M> for Cross {
    type State = ();

    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let width = 1.5;
        // The ends stay inside the bounds, with room for the round caps.
        let (a, b) = (width / 2.0, bounds.width - width / 2.0);
        let stroke = Stroke::default()
            .with_width(width)
            .with_color((self.color)(theme))
            .with_line_cap(LineCap::Round);
        frame.stroke(&Path::line(Point::new(a, a), Point::new(b, b)), stroke);
        frame.stroke(&Path::line(Point::new(b, a), Point::new(a, b)), stroke);
        vec![frame.into_geometry()]
    }
}
