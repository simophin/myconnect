//! An indeterminate progress bar: a segment that sweeps across a track for
//! as long as the bar is shown. iced's progress bar only shows an amount.
//!
//! The bar keeps its own clock in the widget tree and asks for the next
//! frame while it is drawn, so the pages that show it need no timer.

use std::time::Duration;

use iced::{
    Background, Border, Element, Length, Rectangle, Size, Theme,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
        widget::{Tree, tree},
    },
    time::Instant,
    window,
};

/// How long one sweep takes.
const PERIOD: Duration = Duration::from_millis(1400);
/// The segment's share of the track.
const SEGMENT: f32 = 0.35;

/// A bar `width` wide and `height` high that shows something is under way.
pub fn activity_bar(width: impl Into<Length>, height: f32) -> ActivityBar {
    ActivityBar {
        width: width.into(),
        height,
    }
}

pub struct ActivityBar {
    width: Length,
    height: f32,
}

/// When the bar first drew, and the last frame's time.
#[derive(Default)]
struct State {
    started: Option<Instant>,
    now: Option<Instant>,
}

impl State {
    /// Where the sweep is, from 0 to 1. It starts halfway, so the segment
    /// shows from the first frame (and in snapshots, which have no clock).
    fn phase(&self) -> f32 {
        let (Some(started), Some(now)) = (self.started, self.now) else {
            return 0.5;
        };
        let elapsed = now.saturating_duration_since(started).as_secs_f32();
        (elapsed / PERIOD.as_secs_f32() + 0.5).fract()
    }
}

impl<Message, Renderer: renderer::Renderer> Widget<Message, Theme, Renderer> for ActivityBar {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, Length::Fixed(self.height))
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, self.width, self.height)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        if let iced::Event::Window(window::Event::RedrawRequested(now)) = event {
            let state = tree.state.downcast_mut::<State>();
            state.started.get_or_insert(*now);
            state.now = Some(*now);
            shell.request_redraw();
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let palette = theme.extended_palette();
        let border = Border::default().rounded(self.height / 2.0);
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border,
                ..renderer::Quad::default()
            },
            Background::Color(palette.background.strong.color),
        );

        // The segment enters from the left and leaves on the right, clipped
        // to the track.
        let phase = tree.state.downcast_ref::<State>().phase();
        let segment = bounds.width * SEGMENT;
        let start = -segment + phase * (bounds.width + segment);
        let left = start.max(0.0);
        let right = (start + segment).min(bounds.width);
        if right > left {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: bounds.x + left,
                        width: right - left,
                        ..bounds
                    },
                    border,
                    ..renderer::Quad::default()
                },
                Background::Color(palette.primary.base.color),
            );
        }
    }
}

impl<'a, Message: 'a, Renderer: renderer::Renderer + 'a> From<ActivityBar>
    for Element<'a, Message, Theme, Renderer>
{
    fn from(bar: ActivityBar) -> Self {
        Element::new(bar)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sweep_repeats() {
        let started = Instant::now();
        let at = |millis| State {
            started: Some(started),
            now: Some(started + Duration::from_millis(millis)),
        };
        assert!((State::default().phase() - 0.5).abs() < 1e-3);
        assert!((at(0).phase() - 0.5).abs() < 1e-3);
        assert!((at(350).phase() - 0.75).abs() < 1e-3);
        assert!((at(1400 + 1050).phase() - 0.25).abs() < 1e-3);
    }
}
