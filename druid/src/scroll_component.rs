// Copyright 2020 The Druid Authors.
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

//! A component for embedding in another widget to provide consistant and
//! extendable scrolling behavior

use std::f64::INFINITY;
use std::time::Duration;

use crate::kurbo::{Affine, Point, Rect, RoundedRect, Size, Vec2};
use crate::theme;
use crate::{
    BoxConstraints, Env, Event, EventCtx, LifeCycle, LifeCycleCtx, PaintCtx, Region, RenderContext,
    TimerToken,
};

pub const SCROLLBAR_MIN_SIZE: f64 = 45.0;

#[derive(Debug, Copy, Clone)]
pub enum ScrollDirection {
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

#[derive(Debug, Copy, Clone)]
pub enum BarHoveredState {
    None,
    Vertical,
    Horizontal,
}

impl BarHoveredState {
    pub fn is_hovered(&self) -> bool {
        match self {
            BarHoveredState::Vertical | BarHoveredState::Horizontal => true,
            _ => false,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum BarHeldState {
    None,
    /// Vertical scrollbar is being dragged. Contains an `f64` with
    /// the initial y-offset of the dragging input
    Vertical(f64),
    /// Horizontal scrollbar is being dragged. Contains an `f64` with
    /// the initial x-offset of the dragging input
    Horizontal(f64),
}

#[derive(Debug, Copy, Clone)]
pub struct ScrollbarsState {
    pub opacity: f64,
    pub timer_id: TimerToken,
    pub hovered: BarHoveredState,
    pub held: BarHeldState,
}

impl Default for ScrollbarsState {
    fn default() -> Self {
        Self {
            opacity: 0.0,
            timer_id: TimerToken::INVALID,
            hovered: BarHoveredState::None,
            held: BarHeldState::None,
        }
    }
}

impl ScrollbarsState {
    /// true if either scrollbar is currently held down/being dragged
    pub fn are_held(&self) -> bool {
        match self.held {
            BarHeldState::None => false,
            _ => true,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct ScrollComponent {
    pub content_size: Size,
    pub scroll_offset: Vec2,
    pub direction: ScrollDirection,
    pub scrollbars: ScrollbarsState,
}

impl ScrollComponent {
    pub fn new() -> ScrollComponent {
        ScrollComponent {
            content_size: Default::default(),
            scroll_offset: Vec2::new(0.0, 0.0),
            direction: ScrollDirection::All,
            scrollbars: ScrollbarsState::default(),
        }
    }

    /// Update the scroll.
    ///
    /// Returns `true` if the scroll has been updated.
    pub fn scroll(&mut self, delta: Vec2, size: Size) -> bool {
        let mut offset = self.scroll_offset + delta;
        offset.x = offset.x.min(self.content_size.width - size.width).max(0.0);
        offset.y = offset
            .y
            .min(self.content_size.height - size.height)
            .max(0.0);
        if (offset - self.scroll_offset).hypot2() > 1e-12 {
            self.scroll_offset = offset;
            true
        } else {
            false
        }
    }

    /// Makes the scrollbars visible, and resets the fade timer.
    pub fn reset_scrollbar_fade<F>(&mut self, request_timer: F, env: &Env)
    where
        F: FnOnce(Duration) -> TimerToken,
    {
        // Display scroll bars and schedule their disappearance
        self.scrollbars.opacity = env.get(theme::SCROLLBAR_MAX_OPACITY);
        let fade_delay = env.get(theme::SCROLLBAR_FADE_DELAY);
        let deadline = Duration::from_millis(fade_delay);
        self.scrollbars.timer_id = request_timer(deadline);
    }

    pub fn calc_vertical_bar_bounds(&self, viewport: Rect, env: &Env) -> Rect {
        let bar_width = env.get(theme::SCROLLBAR_WIDTH);
        let bar_pad = env.get(theme::SCROLLBAR_PAD);

        let percent_visible = viewport.height() / self.content_size.height;
        let percent_scrolled =
            self.scroll_offset.y / (self.content_size.height - viewport.height());

        let length = (percent_visible * viewport.height()).ceil();
        let length = length.max(SCROLLBAR_MIN_SIZE);

        let vertical_padding = bar_pad + bar_pad + bar_width;

        let top_y_offset =
            ((viewport.height() - length - vertical_padding) * percent_scrolled).ceil();
        let bottom_y_offset = top_y_offset + length;

        let x0 = self.scroll_offset.x + viewport.width() - bar_width - bar_pad;
        let y0 = self.scroll_offset.y + top_y_offset + bar_pad;

        let x1 = self.scroll_offset.x + viewport.width() - bar_pad;
        let y1 = self.scroll_offset.y + bottom_y_offset;

        Rect::new(x0, y0, x1, y1)
    }

    pub fn calc_horizontal_bar_bounds(&self, viewport: Rect, env: &Env) -> Rect {
        let bar_width = env.get(theme::SCROLLBAR_WIDTH);
        let bar_pad = env.get(theme::SCROLLBAR_PAD);

        let percent_visible = viewport.width() / self.content_size.width;
        let percent_scrolled = self.scroll_offset.x / (self.content_size.width - viewport.width());

        let length = (percent_visible * viewport.width()).ceil();
        let length = length.max(SCROLLBAR_MIN_SIZE);

        let horizontal_padding = bar_pad + bar_pad + bar_width;

        let left_x_offset =
            ((viewport.width() - length - horizontal_padding) * percent_scrolled).ceil();
        let right_x_offset = left_x_offset + length;

        let x0 = self.scroll_offset.x + left_x_offset + bar_pad;
        let y0 = self.scroll_offset.y + viewport.height() - bar_width - bar_pad;

        let x1 = self.scroll_offset.x + right_x_offset;
        let y1 = self.scroll_offset.y + viewport.height() - bar_pad;

        Rect::new(x0, y0, x1, y1)
    }

    /// Draw scroll bars.
    pub fn draw_bars(&self, ctx: &mut PaintCtx, viewport: Rect, env: &Env) {
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
        if viewport.height() < self.content_size.height {
            let bounds = self
                .calc_vertical_bar_bounds(viewport, env)
                .inset(-edge_width / 2.0);
            let rect = RoundedRect::from_rect(bounds, radius);
            ctx.render_ctx.fill(rect, &brush);
            ctx.render_ctx.stroke(rect, &border_brush, edge_width);
        }

        // Horizontal bar
        if viewport.width() < self.content_size.width {
            let bounds = self
                .calc_horizontal_bar_bounds(viewport, env)
                .inset(-edge_width / 2.0);
            let rect = RoundedRect::from_rect(bounds, radius);
            ctx.render_ctx.fill(rect, &brush);
            ctx.render_ctx.stroke(rect, &border_brush, edge_width);
        }
    }

    pub fn point_hits_vertical_bar(&self, viewport: Rect, pos: Point, env: &Env) -> bool {
        if viewport.height() < self.content_size.height {
            // Stretch hitbox to edge of widget
            let mut bounds = self.calc_vertical_bar_bounds(viewport, env);
            bounds.x1 = self.scroll_offset.x + viewport.width();
            bounds.contains(pos)
        } else {
            false
        }
    }

    pub fn point_hits_horizontal_bar(&self, viewport: Rect, pos: Point, env: &Env) -> bool {
        if viewport.width() < self.content_size.width {
            // Stretch hitbox to edge of widget
            let mut bounds = self.calc_horizontal_bar_bounds(viewport, env);
            bounds.y1 = self.scroll_offset.y + viewport.height();
            bounds.contains(pos)
        } else {
            false
        }
    }

    pub fn filter_event(&mut self, ctx: &mut EventCtx, event: &Event, env: &Env) -> bool {
        let size = ctx.size();
        let viewport = Rect::from_origin_size(Point::ORIGIN, size);

        let scrollbar_is_hovered = match event {
            Event::MouseMove(e) | Event::MouseUp(e) | Event::MouseDown(e) => {
                let offset_pos = e.pos + self.scroll_offset;
                self.point_hits_vertical_bar(viewport, offset_pos, env)
                    || self.point_hits_horizontal_bar(viewport, offset_pos, env)
            }
            _ => false,
        };

        if self.scrollbars.are_held() {
            // if we're dragging a scrollbar
            match event {
                Event::MouseMove(event) => {
                    match self.scrollbars.held {
                        BarHeldState::Vertical(offset) => {
                            let scale_y = viewport.height() / self.content_size.height;
                            let bounds = self.calc_vertical_bar_bounds(viewport, env);
                            let mouse_y = event.pos.y + self.scroll_offset.y;
                            let delta = mouse_y - bounds.y0 - offset;
                            self.scroll(Vec2::new(0f64, (delta / scale_y).ceil()), size);
                        }
                        BarHeldState::Horizontal(offset) => {
                            let scale_x = viewport.width() / self.content_size.width;
                            let bounds = self.calc_horizontal_bar_bounds(viewport, env);
                            let mouse_x = event.pos.x + self.scroll_offset.x;
                            let delta = mouse_x - bounds.x0 - offset;
                            self.scroll(Vec2::new((delta / scale_x).ceil(), 0f64), size);
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
                        self.reset_scrollbar_fade(|d| ctx.request_timer(d), env);
                    }
                }
                _ => (), // other events are a noop
            }
        } else if scrollbar_is_hovered {
            // if we're over a scrollbar but not dragging
            match event {
                Event::MouseMove(event) => {
                    let offset_pos = event.pos + self.scroll_offset;
                    if self.point_hits_vertical_bar(viewport, offset_pos, env) {
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

                    if self.point_hits_vertical_bar(viewport, pos, env) {
                        ctx.set_active(true);
                        self.scrollbars.held = BarHeldState::Vertical(
                            pos.y - self.calc_vertical_bar_bounds(viewport, env).y0,
                        );
                    } else if self.point_hits_horizontal_bar(viewport, pos, env) {
                        ctx.set_active(true);
                        self.scrollbars.held = BarHeldState::Horizontal(
                            pos.x - self.calc_horizontal_bar_bounds(viewport, env).x0,
                        );
                    }
                }
                // if the mouse was downed elsewhere, moved over a scroll bar and released: noop.
                Event::MouseUp(_) => (),
                _ => unreachable!(),
            }
        } else {
            match event {
                Event::MouseMove(_) => {
                    // if we have just stopped hovering
                    if self.scrollbars.hovered.is_hovered() && !scrollbar_is_hovered {
                        self.scrollbars.hovered = BarHoveredState::None;
                        self.reset_scrollbar_fade(|d| ctx.request_timer(d), env);
                    }
                }
                Event::Timer(id) if *id == self.scrollbars.timer_id => {
                    // Schedule scroll bars animation
                    ctx.request_anim_frame();
                    self.scrollbars.timer_id = TimerToken::INVALID;
                }
                _ => (),
            }

            return false;
        }

        true
    }

    pub fn check_and_scroll(&mut self, ctx: &mut EventCtx, event: &Event, env: &Env) {
        if !ctx.is_handled() {
            if let Event::Wheel(mouse) = event {
                if self.scroll(mouse.wheel_delta, ctx.size()) {
                    ctx.request_paint();
                    ctx.set_handled();
                    self.reset_scrollbar_fade(|d| ctx.request_timer(d), env);
                }
            }
        }
    }

    pub fn filter_lifecycle(
        &mut self,
        ctx: &mut LifeCycleCtx,
        event: &LifeCycle,
        env: &Env,
    ) -> bool {
        match event {
            LifeCycle::AnimFrame(interval) => {
                // Guard by the timer id being invalid, otherwise the scroll bars would fade
                // immediately if some other widget started animating.
                if self.scrollbars.timer_id == TimerToken::INVALID {
                    // Animate scroll bars opacity
                    let diff = 2.0 * (*interval as f64) * 1e-9;
                    self.scrollbars.opacity -= diff;
                    if self.scrollbars.opacity > 0.0 {
                        ctx.request_anim_frame();
                    }

                    return true;
                }
            }
            // Show the scrollbars any time our size changes
            LifeCycle::Size(_) => {
                self.reset_scrollbar_fade(|d| ctx.request_timer(d), &env);
                return true;
            }
            _ => (),
        }

        false
    }

    pub fn draw_content(
        self,
        ctx: &mut PaintCtx,
        env: &Env,
        f: impl FnOnce(Region, &mut PaintCtx),
    ) {
        let viewport = ctx.size().to_rect();
        ctx.with_save(|ctx| {
            ctx.clip(viewport);
            ctx.transform(Affine::translate(-self.scroll_offset));

            let visible = ctx.region().to_rect() + self.scroll_offset;
            f(visible.into(), ctx);

            self.draw_bars(ctx, viewport, env);
        });
    }
}
