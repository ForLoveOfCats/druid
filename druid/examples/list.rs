// Copyright 2019 The Druid Authors.
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

//! Demos basic list widget and list manipulations.

use druid::im::{vector, Vector};
use druid::lens::{self, LensExt};
use druid::widget::{Button, CrossAxisAlignment, Flex, Label, List};
use druid::{
    AppLauncher, Color, Data, Lens, LocalizedString, UnitPoint, Widget, WidgetExt, WindowDesc,
};

#[derive(Clone, Data, Lens)]
struct AppData {
    left: Vector<u32>,
    right: Vector<u32>,
}

pub fn main() {
    let main_window = WindowDesc::new(ui_builder)
        .title(LocalizedString::new("list-demo-window-title").with_placeholder("List Demo"));
    // Set our initial data
    let data = AppData {
        left: vector![1, 2],
        right: vector![1, 2, 3],
    };
    AppLauncher::with_window(main_window)
        .use_simple_logger()
        .launch(data)
        .expect("launch failed");
}

fn ui_builder() -> impl Widget<AppData> {
    let mut root = Flex::column();

    // Build a button to add children to both lists
    root.add_child(
        Button::new("Add")
            .on_click(|_, data: &mut AppData, _| {
                // Add child to left list
                let value = data.left.len() + 1;
                data.left.push_back(value as u32);

                // Add child to right list
                let value = data.right.len() + 1;
                data.right.push_back(value as u32);
            })
            .fix_height(30.0)
            .expand_width(),
    );

    let mut lists = Flex::row().cross_axis_alignment(CrossAxisAlignment::Start);

    // Build a simple list
    lists.add_flex_child(
        List::new(|| {
            Label::new(|item: &u32, _env: &_| format!("List item #{}", item))
                .align_vertical(UnitPoint::LEFT)
                .padding(10.0)
                .expand()
                .height(50.0)
                .background(Color::rgb(0.5, 0.5, 0.5))
        })
        .lens(AppData::left),
        1.0,
    );

    // Build a list with shared data
    lists.add_flex_child(
        List::new(|| {
            Flex::row()
                .with_child(
                    Label::new(|(_, item): &(Vector<u32>, u32), _env: &_| {
                        format!("List item #{}", item)
                    })
                    .align_vertical(UnitPoint::LEFT),
                )
                .with_flex_spacer(1.0)
                .with_child(
                    Button::new("Delete")
                        .on_click(|_ctx, (shared, item): &mut (Vector<u32>, u32), _env| {
                            // We have access to both child's data and shared data.
                            // Remove element from right list.
                            shared.retain(|v| v != item);
                        })
                        .fix_size(80.0, 20.0)
                        .align_vertical(UnitPoint::CENTER),
                )
                .padding(10.0)
                .background(Color::rgb(0.5, 0.0, 0.5))
                .fix_height(50.0)
        })
        .lens(lens::Id.map(
            // Expose shared data with children data
            |d: &AppData| (d.right.clone(), d.right.clone()),
            |d: &mut AppData, x: (Vector<u32>, Vector<u32>)| {
                // If shared data was changed reflect the changes in our AppData
                d.right = x.0
            },
        )),
        1.0,
    );

    root.add_flex_child(lists, 1.0);

    // Mark the widget as needing its layout rects painted
    root.debug_paint_layout()
}
