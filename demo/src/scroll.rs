//! Scrolling the 2D scenes inside their content, with scrollbars; see
//! docs/features/demo.md#scrolling.

use bevy::camera::primitives::Aabb;
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::window::WindowResized;

use crate::{ACCENT, BAR, Camera2dMarker, Current, InScene, TEXT_WEAK};

/// Space kept around the content, in logical pixels.
const MARGIN: f32 = 28.0;
/// Width of a scrollbar.
const THICKNESS: f32 = 10.0;
const MIN_THUMB: f32 = 24.0;
/// Logical pixels per wheel line.
const LINE: f32 = 40.0;
/// Part of the view one page moves.
const PAGE: f32 = 0.9;
/// Height of the status line.
const STATUS: f32 = BAR - 8.0;

/// What an entity without an `Aabb` draws (gizmos), in world space.
#[derive(Component)]
pub struct Extent(pub Rect);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

/// One axis: the camera positions that keep the view in the content, and its scrollbar.
#[derive(Default, Clone, Copy)]
struct Bar {
    lo: f32,
    hi: f32,
    /// Length of the view, the track and the thumb, and where the thumb starts in the
    /// track, in logical pixels.
    view: f32,
    track: f32,
    thumb: f32,
    start: f32,
}

impl Bar {
    /// `lo` and `hi` always hold `0`, where the scene was laid out.
    fn new(lo: f32, hi: f32, view: f32, track: f32) -> Self {
        let (lo, hi) = (lo.min(0.0), hi.max(0.0));
        let thumb = (track * view / (view + hi - lo)).clamp(MIN_THUMB.min(track), track);
        Self {
            lo,
            hi,
            view,
            track,
            thumb,
            start: 0.0,
        }
    }

    fn shown(&self) -> bool {
        self.hi - self.lo > 0.5
    }

    /// How far the camera is along the bar: `0` at the top or left.
    fn offset(&self, axis: Axis, camera: &Transform) -> f32 {
        match axis {
            Axis::Y => self.hi - camera.translation.y,
            Axis::X => camera.translation.x - self.lo,
        }
    }

    fn set_offset(&self, axis: Axis, camera: &mut Transform, offset: f32) {
        let offset = offset.clamp(0.0, self.hi - self.lo);
        match axis {
            Axis::Y => camera.translation.y = self.hi - offset,
            Axis::X => camera.translation.x = self.lo + offset,
        }
    }

    /// Camera units per pixel the thumb moves.
    fn ratio(&self) -> f32 {
        (self.hi - self.lo) / (self.track - self.thumb).max(1.0)
    }
}

#[derive(Resource, Default)]
struct Scroll {
    x: Bar,
    y: Bar,
}

impl Scroll {
    fn bar(&self, axis: Axis) -> &Bar {
        match axis {
            Axis::X => &self.x,
            Axis::Y => &self.y,
        }
    }
}

#[derive(Component)]
struct Track(Axis);

#[derive(Component, Default)]
struct Thumb {
    hovered: bool,
    dragged: bool,
}

pub struct ScrollPlugin;

impl Plugin for ScrollPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Scroll>()
            .add_systems(Startup, spawn_bars)
            .add_systems(
                Update,
                (
                    relayout.before(crate::switch_scene),
                    (measure, scroll, place_bars)
                        .chain()
                        .after(crate::switch_scene),
                ),
            );
    }
}

/// A resized window lays the 2D scene out again.
fn relayout(mut resized: MessageReader<WindowResized>, mut current: ResMut<Current>) {
    if resized.read().count() > 0 && current.shown.is_some_and(|s| !s.is_3d()) {
        current.shown = None;
    }
}

/// The camera's range on each axis from the box around the scene's content.
fn measure(
    current: Res<Current>,
    window: Single<&Window>,
    boxes: Query<(&Aabb, &GlobalTransform), With<InScene>>,
    extents: Query<&Extent, With<InScene>>,
    mut state: ResMut<Scroll>,
) {
    *state = Scroll::default();
    if current.shown.is_none_or(|s| s.is_3d()) {
        return;
    }
    let boxes = boxes.iter().map(|(aabb, t)| {
        let center = t.transform_point(aabb.center.into()).truncate();
        let half = (Vec3::from(aabb.half_extents) * t.scale()).truncate().abs();
        Rect::from_center_half_size(center, half)
    });
    let Some(content) = boxes
        .chain(extents.iter().map(|e| e.0))
        .reduce(|a, b| a.union(b))
    else {
        return;
    };
    let content = content.inflate(MARGIN);
    let (w, h) = (window.width(), window.height());
    // Each shown scrollbar takes room from the other axis's view: settle in two passes.
    let (mut right, mut bottom) = (0.0, 0.0);
    for _ in 0..2 {
        let (top, low) = (h / 2.0 - BAR, -h / 2.0 + STATUS + bottom);
        let (left, high) = (-w / 2.0, w / 2.0 - right);
        state.y = Bar::new(
            content.min.y - low,
            content.max.y - top,
            top - low,
            top - low,
        );
        state.x = Bar::new(
            content.min.x - left,
            content.max.x - high,
            high - left,
            high - left,
        );
        right = if state.y.shown() { THICKNESS } else { 0.0 };
        bottom = if state.x.shown() { THICKNESS } else { 0.0 };
    }
}

/// The wheel and keys move the 2D camera, kept inside the content.
fn scroll(
    current: Res<Current>,
    wheel: Res<AccumulatedMouseScroll>,
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<Scroll>,
    mut camera: Single<&mut Transform, With<Camera2dMarker>>,
) {
    if current.shown.is_none_or(|s| s.is_3d()) {
        return;
    }
    let line = match wheel.unit {
        MouseScrollUnit::Line => LINE,
        MouseScrollUnit::Pixel => 1.0,
    };
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let mut delta = wheel.delta * line;
    if shift {
        delta = Vec2::new(delta.x + delta.y, 0.0);
    }
    let (x, y) = (state.x, state.y);
    let mut down = -delta.y;
    if keys.just_pressed(KeyCode::PageDown) {
        down += y.view * PAGE;
    }
    if keys.just_pressed(KeyCode::PageUp) {
        down -= y.view * PAGE;
    }
    let mut at = y.offset(Axis::Y, &camera) + down;
    if keys.just_pressed(KeyCode::Home) {
        at = 0.0;
    }
    if keys.just_pressed(KeyCode::End) {
        at = y.hi - y.lo;
    }
    y.set_offset(Axis::Y, &mut camera, at);
    let right = x.offset(Axis::X, &camera) - delta.x;
    x.set_offset(Axis::X, &mut camera, right);
    let state = &mut *state;
    for (bar, axis) in [(&mut state.x, Axis::X), (&mut state.y, Axis::Y)] {
        bar.start = if bar.shown() {
            bar.offset(axis, &camera) / bar.ratio()
        } else {
            0.0
        };
    }
}

fn spawn_bars(mut commands: Commands) {
    for axis in [Axis::X, Axis::Y] {
        let node = match axis {
            Axis::Y => Node {
                right: Val::Px(0.0),
                top: Val::Px(BAR),
                width: Val::Px(THICKNESS),
                ..default()
            },
            Axis::X => Node {
                left: Val::Px(0.0),
                bottom: Val::Px(STATUS),
                height: Val::Px(THICKNESS),
                ..default()
            },
        };
        commands
            .spawn((
                Track(axis),
                Node {
                    position_type: PositionType::Absolute,
                    ..node
                },
                BackgroundColor(Color::BLACK.with_alpha(0.25)),
                Visibility::Hidden,
            ))
            .observe(move |press: On<PointerPress>, state: Res<Scroll>, mut camera: Single<&mut Transform, With<Camera2dMarker>>| {
                // A click beside the thumb moves one page toward it.
                let bar = state.bar(axis);
                let at = match axis {
                    Axis::Y => press.pointer.position.y - BAR,
                    Axis::X => press.pointer.position.x,
                };
                let page = bar.view * PAGE;
                let offset = bar.offset(axis, &camera);
                if at < bar.start {
                    bar.set_offset(axis, &mut camera, offset - page);
                } else if at > bar.start + bar.thumb {
                    bar.set_offset(axis, &mut camera, offset + page);
                }
            })
            .with_children(|track| {
                track
                    .spawn((
                        Thumb::default(),
                        Node {
                            position_type: PositionType::Absolute,
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            border_radius: BorderRadius::all(Val::Px(THICKNESS / 2.0)),
                            ..default()
                        },
                        BackgroundColor(TEXT_WEAK),
                    ))
                    .observe(move |drag: On<PointerDrag>, state: Res<Scroll>, mut camera: Single<&mut Transform, With<Camera2dMarker>>| {
                        let bar = state.bar(axis);
                        let moved = match axis {
                            Axis::Y => drag.delta.y,
                            Axis::X => drag.delta.x,
                        };
                        let offset = bar.offset(axis, &camera) + moved * bar.ratio();
                        bar.set_offset(axis, &mut camera, offset);
                    })
                    .observe(|over: On<PointerOver>, mut thumbs: Query<&mut Thumb>| {
                        if let Ok(mut thumb) = thumbs.get_mut(over.entity) {
                            thumb.hovered = true;
                        }
                    })
                    .observe(|out: On<PointerOut>, mut thumbs: Query<&mut Thumb>| {
                        if let Ok(mut thumb) = thumbs.get_mut(out.entity) {
                            thumb.hovered = false;
                        }
                    })
                    .observe(|start: On<PointerDragStart>, mut thumbs: Query<&mut Thumb>| {
                        if let Ok(mut thumb) = thumbs.get_mut(start.entity) {
                            thumb.dragged = true;
                        }
                    })
                    .observe(|end: On<PointerDragEnd>, mut thumbs: Query<&mut Thumb>| {
                        if let Ok(mut thumb) = thumbs.get_mut(end.entity) {
                            thumb.dragged = false;
                        }
                    });
            });
    }
}

/// Shows the scrollbars an axis needs, the thumb where the view is.
fn place_bars(
    state: Res<Scroll>,
    mut tracks: Query<(&Track, &mut Node, &mut Visibility, &Children)>,
    mut thumbs: Query<(&Thumb, &mut Node, &mut BackgroundColor), Without<Track>>,
) {
    for (track, mut node, mut visibility, children) in &mut tracks {
        let bar = state.bar(track.0);
        *visibility = if bar.shown() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        match track.0 {
            Axis::Y => node.height = Val::Px(bar.track),
            Axis::X => node.width = Val::Px(bar.track),
        }
        for child in children {
            let Ok((thumb, mut node, mut color)) = thumbs.get_mut(*child) else {
                continue;
            };
            match track.0 {
                Axis::Y => {
                    node.top = Val::Px(bar.start);
                    node.height = Val::Px(bar.thumb);
                }
                Axis::X => {
                    node.left = Val::Px(bar.start);
                    node.width = Val::Px(bar.thumb);
                }
            }
            color.0 = if thumb.hovered || thumb.dragged {
                ACCENT
            } else {
                TEXT_WEAK
            };
        }
    }
}
