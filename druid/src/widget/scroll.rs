// Copyright 2019 The xi-editor Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! A container that scrolls its contents.

use std::f64::INFINITY;
use std::time::{Duration, Instant};

use crate::kurbo::{Affine, Point, Rect, RoundedRect, Size, Vec2};
use crate::theme;
use crate::{
    BoxConstraints, Data, Env, Event, EventCtx, LayoutCtx, LifeCycle, LifeCycleCtx, PaintCtx,
    RenderContext, TimerToken, UpdateCtx, Widget, WidgetPod,
};

const SCROLLBAR_MIN_SIZE: f64 = 45.0;

#[derive(PartialEq)]
enum ScrollbarStyle {
    Overlay,
    Inlay,
}

#[derive(Debug, Clone)]
enum ScrollDirection {
    Horizontal,
    Vertical,
    All,
}

impl ScrollDirection {
    /// Return the maximum size the container can be given
    /// its scroll direction and box constraints.
    /// In practice vertical scrolling will be width limited to
    /// box constraints and horizontal will be height limited.
    pub fn max_size(&self, bc: &BoxConstraints) -> Size {
        match self {
            ScrollDirection::Horizontal => Size::new(INFINITY, bc.max().height),
            ScrollDirection::Vertical => Size::new(bc.max().width, INFINITY),
            ScrollDirection::All => Size::new(INFINITY, INFINITY),
        }
    }
}

enum BarHoveredState {
    None,
    Vertical,
    Horizontal,
}

impl BarHoveredState {
    fn is_hovered(&self) -> bool {
        match self {
            BarHoveredState::Vertical | BarHoveredState::Horizontal => true,
            _ => false,
        }
    }
}

enum BarHeldState {
    None,
    /// Vertical scrollbar is being dragged. Contains an `f64` with
    /// the initial y-offset of the dragging input
    Vertical(f64),
    /// Horizontal scrollbar is being dragged. Contains an `f64` with
    /// the initial x-offset of the dragging input
    Horizontal(f64),
}

struct ScrollbarsState {
    opacity: f64,
    timer_id: TimerToken,
    hovered: BarHoveredState,
    held: BarHeldState,
    vertical_required: bool,
    horizontal_required: bool,
}

impl Default for ScrollbarsState {
    fn default() -> Self {
        Self {
            opacity: 0.0,
            timer_id: TimerToken::INVALID,
            hovered: BarHoveredState::None,
            held: BarHeldState::None,
            vertical_required: false,
            horizontal_required: false,
        }
    }
}

impl ScrollbarsState {
    /// true if either scrollbar is currently held down/being dragged
    fn are_held(&self) -> bool {
        match self.held {
            BarHeldState::None => false,
            _ => true,
        }
    }
}

fn calc_track_width(env: &Env) -> f64 {
    let bar_width = env.get(theme::SCROLLBAR_WIDTH);
    let bar_pad = env.get(theme::SCROLLBAR_PAD);
    bar_pad + bar_width + bar_pad
}

/// A container that scrolls its contents.
///
/// This container holds a single child, and uses the wheel to scroll it
/// when the child's bounds are larger than the viewport.
///
/// The child is laid out with completely unconstrained layout bounds.
pub struct Scroll<T, W> {
    child: WidgetPod<T, W>,
    child_size: Size,
    scroll_offset: Vec2,
    scrollbar_style: ScrollbarStyle,
    direction: ScrollDirection,
    scrollbars: ScrollbarsState,
}

impl<T, W: Widget<T>> Scroll<T, W> {
    /// Create a new scroll container.
    ///
    /// This method will allow scrolling in all directions if child's bounds
    /// are larger than the viewport. Use [vertical](#method.vertical)
    /// and [horizontal](#method.horizontal) methods to limit scroll behavior.
    pub fn new(child: W) -> Scroll<T, W> {
        Scroll {
            child: WidgetPod::new(child),
            child_size: Default::default(),
            scroll_offset: Vec2::new(0.0, 0.0),
            scrollbar_style: ScrollbarStyle::Overlay,
            direction: ScrollDirection::All,
            scrollbars: ScrollbarsState::default(),
        }
    }

    //TODO(ForLoveOfCats) Have a seperate constructor for each style
    pub fn inlay_scrollbars(mut self) -> Self {
        self.scrollbar_style = ScrollbarStyle::Inlay;
        self
    }

    /// Limit scroll behavior to allow only vertical scrolling (Y-axis).
    /// The child is laid out with constrained width and infinite height.
    pub fn vertical(mut self) -> Self {
        self.direction = ScrollDirection::Vertical;
        self
    }

    /// Limit scroll behavior to allow only horizontal scrolling (X-axis).
    /// The child is laid out with constrained height and infinite width.
    pub fn horizontal(mut self) -> Self {
        self.direction = ScrollDirection::Horizontal;
        self
    }

    /// Returns a reference to the child widget.
    pub fn child(&self) -> &W {
        self.child.widget()
    }

    /// Returns a mutable reference to the child widget.
    pub fn child_mut(&mut self) -> &mut W {
        self.child.widget_mut()
    }

    /// Update the scroll.
    ///
    /// Returns `true` if the scroll has been updated.
    pub fn scroll(&mut self, delta: Vec2, size: Size) -> bool {
        let mut offset = self.scroll_offset + delta;
        offset.x = offset.x.min(self.child_size.width - size.width).max(0.0);
        offset.y = offset.y.min(self.child_size.height - size.height).max(0.0);
        if (offset - self.scroll_offset).hypot2() > 1e-12 {
            self.scroll_offset = offset;
            true
        } else {
            false
        }
    }

    /// Makes the scrollbars visible, and resets the fade timer.
    pub fn reset_scrollbar_fade(&mut self, ctx: &mut EventCtx, env: &Env) {
        // Display scroll bars and if overlay style schedule their disappearance
        self.scrollbars.opacity = env.get(theme::SCROLLBAR_MAX_OPACITY);

        if self.scrollbar_style == ScrollbarStyle::Overlay {
            let fade_delay = env.get(theme::SCROLLBAR_FADE_DELAY);
            let deadline = Instant::now() + Duration::from_millis(fade_delay);
            self.scrollbars.timer_id = ctx.request_timer(deadline);
        }
    }

    /// Returns the current scroll offset.
    pub fn offset(&self) -> Vec2 {
        self.scroll_offset
    }

    fn calc_vertical_bar_bounds(&self, viewport: Rect, env: &Env) -> Rect {
        let bar_width = env.get(theme::SCROLLBAR_WIDTH);
        let bar_pad = env.get(theme::SCROLLBAR_PAD);

        let percent_visible = viewport.height() / self.child_size.height;
        let percent_scrolled = self.scroll_offset.y / (self.child_size.height - viewport.height());

        let length = (percent_visible * viewport.height()).ceil();
        let length = length.max(SCROLLBAR_MIN_SIZE);

        let vertical_padding = bar_pad + calc_track_width(env);

        let top_y_offset =
            ((viewport.height() - length - vertical_padding) * percent_scrolled).ceil();
        let bottom_y_offset = top_y_offset + length;

        let x0 = self.scroll_offset.x + viewport.width() - bar_width - bar_pad;
        let y0 = self.scroll_offset.y + top_y_offset + bar_pad;

        let x1 = self.scroll_offset.x + viewport.width() - bar_pad;
        let y1 = self.scroll_offset.y + bottom_y_offset;

        Rect::new(x0, y0, x1, y1)
    }

    fn calc_horizontal_bar_bounds(&self, viewport: Rect, env: &Env) -> Rect {
        let bar_width = env.get(theme::SCROLLBAR_WIDTH);
        let bar_pad = env.get(theme::SCROLLBAR_PAD);

        let percent_visible = viewport.width() / self.child_size.width;
        let percent_scrolled = self.scroll_offset.x / (self.child_size.width - viewport.width());

        let length = (percent_visible * viewport.width()).ceil();
        let length = length.max(SCROLLBAR_MIN_SIZE);

        let horizontal_padding = bar_pad + calc_track_width(env);

        let left_x_offset =
            ((viewport.width() - length - horizontal_padding) * percent_scrolled).ceil();
        let right_x_offset = left_x_offset + length;

        let x0 = self.scroll_offset.x + left_x_offset + bar_pad;
        let y0 = self.scroll_offset.y + viewport.height() - bar_width - bar_pad;

        let x1 = self.scroll_offset.x + right_x_offset;
        let y1 = self.scroll_offset.y + viewport.height() - bar_pad;

        Rect::new(x0, y0, x1, y1)
    }

    fn calc_track_eat(&self, env: &Env) -> Size {
        let track_width = calc_track_width(env);

        Size::new(
            if self.scrollbars.vertical_required {
                track_width
            } else {
                0.
            },
            if self.scrollbars.horizontal_required {
                track_width
            } else {
                0.
            },
        )
    }

    /// Draw scrollbar backgrounds regardless of scrollbar style
    fn draw_scrollbar_background(&self, ctx: &mut PaintCtx, viewport: Rect, env: &Env) {
        let track_width = calc_track_width(env);

        let background_brush = ctx
            .render_ctx
            .solid_brush(env.get(theme::SCROLLBAR_BACKGROUND_COLOR));
        let corner_brush = ctx
            .render_ctx
            .solid_brush(env.get(theme::SCROLLBAR_CORNER_COLOR));

        let vertical_rect = Rect::new(
            self.scroll_offset.x + viewport.width() - track_width,
            self.scroll_offset.y,
            self.scroll_offset.x + viewport.width(),
            self.scroll_offset.y + viewport.height() - track_width,
        );
        let horizontal_rect = Rect::new(
            self.scroll_offset.x,
            self.scroll_offset.y + viewport.height() - track_width,
            self.scroll_offset.x + viewport.width() - track_width,
            self.scroll_offset.y + viewport.height(),
        );
        let corner_rect = Rect::new(
            self.scroll_offset.x + viewport.width() - track_width,
            self.scroll_offset.y + viewport.height() - track_width,
            self.scroll_offset.x + viewport.width(),
            self.scroll_offset.y + viewport.height(),
        );

        if self.scrollbars.vertical_required {
            ctx.render_ctx.fill(vertical_rect, &background_brush);
        }
        if self.scrollbars.horizontal_required {
            ctx.render_ctx.fill(horizontal_rect, &background_brush);
        }
        if self.scrollbars.vertical_required || self.scrollbars.horizontal_required {
            ctx.render_ctx.fill(corner_rect, &corner_brush);
        }
    }

    /// Draw scroll bars.
    fn draw_bars(&self, ctx: &mut PaintCtx, viewport: Rect, env: &Env) {
        if self.scrollbars.opacity <= 0.0 {
            return;
        }

        let brush = ctx.render_ctx.solid_brush(
            env.get(theme::SCROLLBAR_COLOR)
                .with_alpha(self.scrollbars.opacity),
        );
        let border_brush = ctx.render_ctx.solid_brush(
            env.get(theme::SCROLLBAR_BORDER_COLOR)
                .with_alpha(self.scrollbars.opacity),
        );

        let radius = env.get(theme::SCROLLBAR_RADIUS);
        let edge_width = env.get(theme::SCROLLBAR_EDGE_WIDTH);

        // Vertical bar
        if self.scrollbars.vertical_required {
            let bounds = self.calc_vertical_bar_bounds(viewport, &env);
            let rect = RoundedRect::from_rect(bounds, radius);
            ctx.render_ctx.fill(rect, &brush);
            ctx.render_ctx.stroke(rect, &border_brush, edge_width);
        }

        // Horizontal bar
        if self.scrollbars.horizontal_required {
            let bounds = self.calc_horizontal_bar_bounds(viewport, &env);
            let rect = RoundedRect::from_rect(bounds, radius);
            ctx.render_ctx.fill(rect, &brush);
            ctx.render_ctx.stroke(rect, &border_brush, edge_width);
        }
    }

    fn point_hits_vertical_bar(&self, viewport: Rect, pos: Point, env: &Env) -> bool {
        if self.scrollbars.vertical_required {
            // Stretch hitbox to edge of widget
            let mut bounds = self.calc_vertical_bar_bounds(viewport, &env);
            bounds.x1 = self.scroll_offset.x + viewport.width();
            bounds.contains(pos)
        } else {
            false
        }
    }

    fn point_hits_horizontal_bar(&self, viewport: Rect, pos: Point, env: &Env) -> bool {
        if self.scrollbars.horizontal_required {
            // Stretch hitbox to edge of widget
            let mut bounds = self.calc_horizontal_bar_bounds(viewport, &env);
            bounds.y1 = self.scroll_offset.y + viewport.height();
            bounds.contains(pos)
        } else {
            false
        }
    }
}

impl<T: Data, W: Widget<T>> Widget<T> for Scroll<T, W> {
    fn event(&mut self, ctx: &mut EventCtx, event: &Event, data: &mut T, env: &Env) {
        let size = ctx.size();
        let viewport = Rect::from_origin_size(Point::ORIGIN, size);

        let paint_size = match self.scrollbar_style {
            ScrollbarStyle::Overlay => ctx.size(),
            ScrollbarStyle::Inlay => ctx.size() - self.calc_track_eat(env),
        };
        let paint_viewport = Rect::from_origin_size(Point::ORIGIN, paint_size);

        let scrollbar_is_hovered = match event {
            Event::MouseMoved(e) | Event::MouseUp(e) | Event::MouseDown(e) => {
                let offset_pos = e.pos + self.scroll_offset;
                self.point_hits_vertical_bar(viewport, offset_pos, &env)
                    || self.point_hits_horizontal_bar(viewport, offset_pos, &env)
            }
            _ => false,
        };

        if self.scrollbars.are_held() {
            // if we're dragging a scrollbar
            match event {
                Event::MouseMoved(event) => {
                    match self.scrollbars.held {
                        BarHeldState::Vertical(offset) => {
                            let scale_y = paint_viewport.height() / self.child_size.height;
                            let bounds = self.calc_vertical_bar_bounds(viewport, &env);
                            let mouse_y = event.pos.y + self.scroll_offset.y;
                            let delta = mouse_y - bounds.y0 - offset;
                            self.scroll(Vec2::new(0f64, (delta / scale_y).ceil()), paint_size);
                        }
                        BarHeldState::Horizontal(offset) => {
                            let scale_x = paint_viewport.width() / self.child_size.width;
                            let bounds = self.calc_horizontal_bar_bounds(viewport, &env);
                            let mouse_x = event.pos.x + self.scroll_offset.x;
                            let delta = mouse_x - bounds.x0 - offset;
                            self.scroll(Vec2::new((delta / scale_x).ceil(), 0f64), paint_size);
                        }
                        _ => (),
                    }
                    ctx.request_paint();
                }
                Event::MouseUp(_) => {
                    self.scrollbars.held = BarHeldState::None;
                    ctx.set_active(false);

                    if !scrollbar_is_hovered {
                        self.scrollbars.hovered = BarHoveredState::None;
                        self.reset_scrollbar_fade(ctx, &env);
                    }
                }
                _ => (), // other events are a noop
            }
        } else if scrollbar_is_hovered {
            // if we're over a scrollbar but not dragging
            match event {
                Event::MouseMoved(event) => {
                    let offset_pos = event.pos + self.scroll_offset;
                    if self.point_hits_vertical_bar(viewport, offset_pos, &env) {
                        self.scrollbars.hovered = BarHoveredState::Vertical;
                    } else {
                        self.scrollbars.hovered = BarHoveredState::Horizontal;
                    }

                    self.scrollbars.opacity = env.get(theme::SCROLLBAR_MAX_OPACITY);
                    self.scrollbars.timer_id = TimerToken::INVALID; // Cancel any fade out in progress
                    ctx.request_paint();
                }
                Event::MouseDown(event) => {
                    let pos = event.pos + self.scroll_offset;

                    if self.point_hits_vertical_bar(viewport, pos, &env) {
                        ctx.set_active(true);
                        self.scrollbars.held = BarHeldState::Vertical(
                            pos.y - self.calc_vertical_bar_bounds(viewport, &env).y0,
                        );
                    } else if self.point_hits_horizontal_bar(viewport, pos, &env) {
                        ctx.set_active(true);
                        self.scrollbars.held = BarHeldState::Horizontal(
                            pos.x - self.calc_horizontal_bar_bounds(viewport, &env).x0,
                        );
                    }
                }
                // if the mouse was downed elsewhere, moved over a scroll bar and released: noop.
                Event::MouseUp(_) => (),
                _ => unreachable!(),
            }
        } else {
            let force_event = self.child.is_hot() || self.child.is_active();
            let child_event = event.transform_scroll(self.scroll_offset, viewport, force_event);
            if let Some(child_event) = child_event {
                self.child.event(ctx, &child_event, data, env)
            };

            match event {
                Event::MouseMoved(_) => {
                    // if we have just stopped hovering
                    if self.scrollbars.hovered.is_hovered() && !scrollbar_is_hovered {
                        self.scrollbars.hovered = BarHoveredState::None;
                        self.reset_scrollbar_fade(ctx, &env);
                    }
                }
                // Show the scrollbars any time our size changes
                Event::Size(_) => self.reset_scrollbar_fade(ctx, &env),
                Event::Timer(id) if *id == self.scrollbars.timer_id => {
                    // Schedule scroll bars animation
                    ctx.request_anim_frame();
                    self.scrollbars.timer_id = TimerToken::INVALID;
                }
                _ => (),
            }
        }

        if !ctx.is_handled() {
            if let Event::Wheel(wheel) = event {
                if self.scroll(wheel.delta, paint_size) {
                    ctx.request_paint();
                    ctx.set_handled();
                    self.reset_scrollbar_fade(ctx, &env);
                }
            }
        }
    }

    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, data: &T, env: &Env) {
        // Guard by the timer id being invalid, otherwise the scroll bars would fade
        // immediately if some other widgeet started animating.
        if let LifeCycle::AnimFrame(interval) = event {
            if self.scrollbars.timer_id == TimerToken::INVALID {
                // Animate scroll bars opacity
                let diff = 2.0 * (*interval as f64) * 1e-9;
                self.scrollbars.opacity -= diff;
                if self.scrollbars.opacity > 0.0 {
                    ctx.request_anim_frame();
                }
            }
        }
        self.child.lifecycle(ctx, event, data, env)
    }

    fn update(&mut self, ctx: &mut UpdateCtx, _old_data: &T, data: &T, env: &Env) {
        self.child.update(ctx, data, env);
    }

    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &BoxConstraints, data: &T, env: &Env) -> Size {
        bc.debug_check("Scroll");

        let child_bc = BoxConstraints::new(Size::ZERO, self.direction.max_size(bc));
        let size = self.child.layout(ctx, &child_bc, data, env);

        if size.width.is_infinite() {
            log::warn!("Scroll widget's child has an infinite width.");
        }

        if size.height.is_infinite() {
            log::warn!("Scroll widget's child has an infinite height.");
        }

        self.child_size = size;
        self.child
            .set_layout_rect(Rect::from_origin_size(Point::ORIGIN, size));
        let self_size = bc.constrain(self.child_size);
        let _ = self.scroll(Vec2::new(0.0, 0.0), self_size);

        self.scrollbars.vertical_required = self_size.height < self.child_size.height;
        self.scrollbars.horizontal_required = self_size.width < self.child_size.width;

        let track_width = calc_track_width(env);
        if self.scrollbars.horizontal_required {
            self.scrollbars.vertical_required =
                self_size.height - track_width < self.child_size.height;
        }
        if self.scrollbars.vertical_required {
            self.scrollbars.horizontal_required =
                self_size.width - track_width < self.child_size.width;
        }

        self_size
    }

    fn paint(&mut self, ctx: &mut PaintCtx, data: &T, env: &Env) {
        let viewport = ctx.size().to_rect();
        let paint_viewport = match self.scrollbar_style {
            ScrollbarStyle::Overlay => viewport,
            ScrollbarStyle::Inlay => (ctx.size() - self.calc_track_eat(env)).to_rect(),
        };

        ctx.with_save(|ctx| {
            ctx.clip(paint_viewport);
            ctx.transform(Affine::translate(-self.scroll_offset));

            let visible = paint_viewport.with_origin(self.scroll_offset.to_point());
            ctx.with_child_ctx(visible, |ctx| self.child.paint(ctx, data, env));
        });

        ctx.with_save(|ctx| {
            ctx.clip(viewport);
            ctx.transform(Affine::translate(-self.scroll_offset));

            if self.scrollbar_style == ScrollbarStyle::Inlay {
                self.draw_scrollbar_background(ctx, viewport, env);
            }
            self.draw_bars(ctx, viewport, env);
        });
    }
}
