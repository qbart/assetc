//! Locator markers and the move / rotate gizmo of the model preview, as screen geometry; see
//! docs/features/locators.md#3d-view.

use bevy::math::{Affine3A, Quat, Vec3};
use bevy::prelude::Transform;
use bevy_egui::egui::{Pos2, Rect, Vec2, pos2};

use crate::amod::Locator;
use crate::preview3d::{FOV_DEGREES, Orbit};

/// Screen length of a Move arrow, and radius of a Rotate circle, in points.
pub const ARROW: f32 = 80.0;
pub const RING: f32 = 70.0;
/// Segments of a Rotate circle.
const RING_SEGMENTS: usize = 48;
/// Length of a marker's axis lines.
pub const MARKER_AXIS: f32 = 16.0;
/// How near the pointer must be to a handle, to a marker's dot, and to the center dot.
pub const HIT: f32 = 6.0;
pub const PICK: f32 = 8.0;
pub const CENTER_HIT: f32 = 9.0;

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Mode {
    #[default]
    Move,
    Rotate,
    /// Clicking the model places the chosen locator; no gizmo.
    Place,
}

/// A part of the gizmo that can be dragged.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Handle {
    /// A Move arrow, by axis.
    Arrow(usize),
    /// The Move center dot.
    Center,
    /// A Rotate circle, by axis.
    Ring(usize),
}

/// The preview camera as the canvas sees it: world points to canvas points.
pub struct ViewCamera {
    rect: Rect,
    camera: Transform,
    to_view: Affine3A,
    tan_half: f32,
}

impl ViewCamera {
    pub fn new(rect: Rect, orbit: &Orbit) -> Self {
        let camera = orbit.transform();
        Self {
            rect,
            to_view: camera.compute_affine().inverse(),
            camera,
            tan_half: (FOV_DEGREES.to_radians() / 2.0).tan(),
        }
    }

    /// Where `p` shows in the canvas; `None` behind the camera.
    pub fn project(&self, p: Vec3) -> Option<Pos2> {
        let v = self.to_view.transform_point3(p);
        if v.z > -1e-4 {
            return None;
        }
        let aspect = self.rect.width() / self.rect.height().max(1.0);
        let (x, y) = (
            v.x / (-v.z * self.tan_half * aspect),
            v.y / (-v.z * self.tan_half),
        );
        let c = self.rect.center();
        Some(pos2(
            c.x + x * self.rect.width() / 2.0,
            c.y - y * self.rect.height() / 2.0,
        ))
    }

    /// The ray through canvas point `p`: origin and unit direction, in world space.
    pub fn ray(&self, p: Pos2) -> (Vec3, Vec3) {
        let aspect = self.rect.width() / self.rect.height().max(1.0);
        let c = self.rect.center();
        let x = (p.x - c.x) / (self.rect.width() / 2.0);
        let y = -(p.y - c.y) / (self.rect.height() / 2.0);
        let view = Vec3::new(x * self.tan_half * aspect, y * self.tan_half, -1.0);
        (
            self.camera.translation,
            (self.camera.rotation * view).normalize(),
        )
    }

    /// World units per canvas point across the view at `p`'s depth.
    pub fn world_per_point(&self, p: Vec3) -> f32 {
        let depth = -self.to_view.transform_point3(p).z;
        2.0 * depth.max(1e-4) * self.tan_half / self.rect.height().max(1.0)
    }
}

/// A locator's transform relative to its parent.
pub fn local(locator: &Locator) -> Affine3A {
    let rotation = Quat::from_array(locator.rotation);
    let rotation = if rotation.length_squared() > 0.0 {
        rotation.normalize()
    } else {
        Quat::IDENTITY
    };
    Affine3A::from_scale_rotation_translation(
        Vec3::from(locator.scale),
        rotation,
        Vec3::from(locator.translation),
    )
}

/// Sets a locator's transform relative to its parent.
pub fn set_local(locator: &mut Locator, local: Affine3A) {
    let (scale, rotation, translation) = local.to_scale_rotation_translation();
    locator.translation = translation.to_array();
    locator.rotation = rotation.normalize().to_array();
    locator.scale = scale.to_array();
}

/// Puts `locator` at world `point` with its Y axis along world `normal`, keeping its scale;
/// see docs/features/locators.md#place-on-surface.
pub fn place(locator: &mut Locator, parent: Affine3A, point: Vec3, normal: Vec3) {
    let world =
        Affine3A::from_rotation_translation(Quat::from_rotation_arc(Vec3::Y, normal), point);
    let (_, rotation, translation) = (parent.inverse() * world).to_scale_rotation_translation();
    locator.translation = translation.to_array();
    locator.rotation = rotation.normalize().to_array();
}

/// `world` reflected across the model-space plane through the origin that is across `axis`
/// (0 X, 1 Y, 2 Z), unmirrored again: `S · T · S` in model space.
pub fn mirrored(model: Affine3A, world: Affine3A, axis: usize) -> Affine3A {
    let mut s = Vec3::ONE;
    s[axis] = -1.0;
    let s = Affine3A::from_scale(s);
    model * s * model.inverse() * world * s
}

/// `name` with its left / right tokens swapped, or `None` when it has none; see
/// docs/features/locators.md#mirroring.
pub fn mirror_name(name: &str) -> Option<String> {
    const WORDS: [(&str, &str); 6] = [
        ("Left", "Right"),
        ("Right", "Left"),
        ("left", "right"),
        ("right", "left"),
        ("LEFT", "RIGHT"),
        ("RIGHT", "LEFT"),
    ];
    let separator = |c: Option<char>| c.is_none_or(|c| matches!(c, '_' | '.' | '-' | ' '));
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len());
    let mut i = 0;
    while i < chars.len() {
        let rest: String = chars[i..].iter().collect();
        if let Some((word, other)) = WORDS.iter().find(|(w, _)| rest.starts_with(w)) {
            out += other;
            i += word.chars().count();
            continue;
        }
        let alone =
            separator(i.checked_sub(1).map(|k| chars[k])) && separator(chars.get(i + 1).copied());
        out.push(match chars[i] {
            'l' if alone => 'r',
            'r' if alone => 'l',
            'L' if alone => 'R',
            'R' if alone => 'L',
            c => c,
        });
        i += 1;
    }
    (out != name).then_some(out)
}

/// The counterpart of `joint` among `joints`: its mirrored name, or the first joint matching
/// it with trailing numbers left out of both; see docs/features/locators.md#mirroring.
pub fn mirror_joint(joint: &str, joints: &[String]) -> Option<String> {
    fn unnumbered(name: &str) -> &str {
        let trimmed = name.trim_end_matches(|c: char| c.is_ascii_digit());
        if trimmed.len() == name.len() {
            return name;
        }
        trimmed.strip_suffix(['_', '.', '-']).unwrap_or(trimmed)
    }
    let mirrored = mirror_name(joint)?;
    if joints.contains(&mirrored) {
        return Some(mirrored);
    }
    joints
        .iter()
        .find(|j| unnumbered(j) == unnumbered(&mirrored))
        .cloned()
}

/// Direction of a transform's axis `i` in world space.
fn axis(world: Affine3A, i: usize) -> Vec3 {
    world.matrix3.col(i).normalize_or_zero().into()
}

/// The gizmo of one locator, as drawn: its center and its handles in canvas points.
pub struct Gizmo {
    pub center: Pos2,
    /// Move arrow tips, by axis; `None` when the axis points nearly at the camera.
    pub arrows: [Option<Pos2>; 3],
    /// Rotate circles, by axis: visible segment runs.
    pub rings: [Vec<Vec<Pos2>>; 3],
}

impl Gizmo {
    /// The gizmo of a locator whose world transform is `world`; `None` when it is behind
    /// the camera.
    pub fn new(camera: &ViewCamera, world: Affine3A, mode: Mode) -> Option<Self> {
        if mode == Mode::Place {
            return None;
        }
        let origin = Vec3::from(world.translation);
        let center = camera.project(origin)?;
        let scale = camera.world_per_point(origin);
        let mut gizmo = Self {
            center,
            arrows: [None; 3],
            rings: Default::default(),
        };
        for i in 0..3 {
            let a = axis(world, i);
            match mode {
                Mode::Move => {
                    gizmo.arrows[i] = camera
                        .project(origin + a * ARROW * scale)
                        .filter(|tip| (*tip - center).length() >= 0.15 * ARROW);
                }
                Mode::Rotate => {
                    let b = a.any_orthonormal_vector();
                    let c = a.cross(b);
                    let mut runs = vec![Vec::new()];
                    for k in 0..=RING_SEGMENTS {
                        let t = k as f32 / RING_SEGMENTS as f32 * std::f32::consts::TAU;
                        let p = origin + (b * t.cos() + c * t.sin()) * RING * scale;
                        match camera.project(p) {
                            Some(p) => runs.last_mut().unwrap().push(p),
                            None => runs.push(Vec::new()),
                        }
                    }
                    runs.retain(|run| run.len() > 1);
                    gizmo.rings[i] = runs;
                }
                Mode::Place => {}
            }
        }
        Some(gizmo)
    }

    /// The handle under `pointer`: the center dot first, then the nearest arrow or circle.
    pub fn hit(&self, pointer: Pos2) -> Option<Handle> {
        let mut best: Option<(f32, Handle)> = None;
        let mut consider = |distance: f32, handle| {
            if distance <= HIT && best.is_none_or(|(d, _)| distance < d) {
                best = Some((distance, handle));
            }
        };
        let arrows = self.arrows.iter().any(Option::is_some);
        if arrows && (pointer - self.center).length() <= CENTER_HIT {
            return Some(Handle::Center);
        }
        for i in 0..3 {
            if let Some(tip) = self.arrows[i] {
                consider(
                    segment_distance(pointer, self.center, tip),
                    Handle::Arrow(i),
                );
            }
            for run in &self.rings[i] {
                for pair in run.windows(2) {
                    consider(segment_distance(pointer, pair[0], pair[1]), Handle::Ring(i));
                }
            }
        }
        best.map(|(_, handle)| handle)
    }
}

/// Distance from `p` to the segment `a`–`b`.
pub fn segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = if ab.length_sq() > 0.0 {
        ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p - (a + ab * t)).length()
}

/// A gizmo drag in progress.
#[derive(Clone, Copy, Debug)]
pub struct Drag {
    pub handle: Handle,
    /// The pointer's last angle around the gizmo's center (Rotate).
    angle: f32,
}

fn angle_around(center: Pos2, p: Pos2) -> f32 {
    let v = p - center;
    v.y.atan2(v.x)
}

impl Drag {
    pub fn start(handle: Handle, gizmo: &Gizmo, pointer: Pos2) -> Self {
        Self {
            handle,
            angle: angle_around(gizmo.center, pointer),
        }
    }

    /// Moves or turns `locator`, whose parent's world transform is `parent`, for the pointer
    /// now at `pointer` after moving by `delta` points.
    pub fn apply(
        &mut self,
        locator: &mut Locator,
        parent: Affine3A,
        camera: &ViewCamera,
        pointer: Pos2,
        delta: Vec2,
    ) {
        let world = parent * local(locator);
        let origin = Vec3::from(world.translation);
        let Some(center) = camera.project(origin) else {
            return;
        };
        let scale = camera.world_per_point(origin);
        let moved = match self.handle {
            Handle::Arrow(i) => {
                let a = axis(world, i);
                let Some(tip) = camera.project(origin + a * ARROW * scale) else {
                    return;
                };
                let along = tip - center;
                if along.length() < 1.0 {
                    return;
                }
                // The arrow's on-screen length stands for ARROW * scale world units.
                let points = delta.dot(along.normalized());
                a * points * ARROW * scale / along.length()
            }
            Handle::Center => {
                let rotation = camera.camera.rotation;
                (rotation * Vec3::X * delta.x - rotation * Vec3::Y * delta.y) * scale
            }
            Handle::Ring(i) => {
                let angle = angle_around(center, pointer);
                let mut turn = angle - self.angle;
                // Across the ±π seam.
                if turn > std::f32::consts::PI {
                    turn -= std::f32::consts::TAU;
                } else if turn < -std::f32::consts::PI {
                    turn += std::f32::consts::TAU;
                }
                self.angle = angle;
                let a = axis(world, i);
                let facing = a.dot(camera.camera.translation - origin) >= 0.0;
                let mirrored = parent.matrix3.determinant() < 0.0;
                // Screen y points down, so a growing angle turns clockwise on screen, which is
                // a negative turn around an axis pointing at the viewer.
                let sign = if facing != mirrored { -1.0 } else { 1.0 };
                let rotation = local(locator).to_scale_rotation_translation().1;
                let mut e = Vec3::ZERO;
                e[i] = 1.0;
                locator.rotation = (rotation * Quat::from_axis_angle(e, sign * turn))
                    .normalize()
                    .to_array();
                return;
            }
        };
        let step = parent.inverse().transform_vector3(moved);
        locator.translation = (Vec3::from(locator.translation) + step).to_array();
    }
}

/// A locator's dot and its axis line ends in the canvas; `None` behind the camera.
pub fn marker(camera: &ViewCamera, world: Affine3A) -> Option<(Pos2, [Option<Pos2>; 3])> {
    let origin = Vec3::from(world.translation);
    let center = camera.project(origin)?;
    let scale = camera.world_per_point(origin);
    Some((
        center,
        std::array::from_fn(|i| camera.project(origin + axis(world, i) * MARKER_AXIS * scale)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_egui::egui::vec2;

    /// An 800 × 600 canvas looking down −Z from 4 units away.
    fn test_camera() -> ViewCamera {
        let orbit = Orbit {
            yaw: 0.0,
            pitch: 0.0,
            distance: 4.0,
        };
        ViewCamera::new(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0)),
            &orbit,
        )
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn projection_centers_the_origin() {
        let camera = test_camera();
        let center = camera.project(Vec3::ZERO).unwrap();
        assert!(near(center.x, 400.0) && near(center.y, 300.0));
        // One point's worth of world moves one point on screen; up is up.
        let s = camera.world_per_point(Vec3::ZERO);
        let up = camera.project(Vec3::Y * 10.0 * s).unwrap();
        assert!(near(up.y, 290.0), "{up:?}");
        let right = camera.project(Vec3::X * 10.0 * s).unwrap();
        assert!(near(right.x, 410.0), "{right:?}");
        // Behind the camera (at z = 4).
        assert!(camera.project(Vec3::Z * 5.0).is_none());
    }

    #[test]
    fn arrows_follow_the_pointer() {
        let camera = test_camera();
        let mut locator = Locator::new("a".into());
        let world = local(&locator);
        let gizmo = Gizmo::new(&camera, world, Mode::Move).unwrap();
        // Z points at the camera: no arrow.
        assert!(gizmo.arrows[2].is_none());
        let tip = gizmo.arrows[0].unwrap();
        assert!(near(tip.x - gizmo.center.x, ARROW));
        assert_eq!(gizmo.hit(gizmo.center), Some(Handle::Center));
        assert_eq!(
            gizmo.hit(gizmo.center + vec2(50.0, 2.0)),
            Some(Handle::Arrow(0))
        );
        assert_eq!(gizmo.hit(gizmo.center + vec2(50.0, 20.0)), None);

        // Dragging the X arrow by 10 points (and sideways, which is ignored) moves 10 points.
        let mut drag = Drag::start(Handle::Arrow(0), &gizmo, tip);
        let s = camera.world_per_point(Vec3::ZERO);
        drag.apply(
            &mut locator,
            Affine3A::IDENTITY,
            &camera,
            tip,
            vec2(10.0, 5.0),
        );
        assert!(
            near(locator.translation[0] / s, 10.0),
            "{:?}",
            locator.translation
        );
        assert!(near(locator.translation[1], 0.0));

        // In a parent scaled by 2, the same drag moves half as far in its space.
        let mut child = Locator::new("b".into());
        let parent = Affine3A::from_scale(Vec3::splat(2.0));
        let mut drag = Drag::start(Handle::Center, &gizmo, gizmo.center);
        drag.apply(&mut child, parent, &camera, gizmo.center, vec2(0.0, -10.0));
        assert!(
            near(child.translation[1] * 2.0 / s, 10.0),
            "{:?}",
            child.translation
        );
    }

    #[test]
    fn rings_turn_with_the_pointer() {
        let camera = test_camera();
        let mut locator = Locator::new("a".into());
        let gizmo = Gizmo::new(&camera, local(&locator), Mode::Rotate).unwrap();
        // The Z circle faces the camera (X and Y are seen edge-on, as lines through the center).
        let d = RING * std::f32::consts::FRAC_1_SQRT_2;
        let start = gizmo.center + vec2(d, -d);
        assert_eq!(gizmo.hit(start), Some(Handle::Ring(2)));
        // Turning the pointer a quarter counter-clockwise on screen turns +Z by a quarter.
        let mut drag = Drag::start(Handle::Ring(2), &gizmo, start);
        let end = gizmo.center + vec2(-d, -d);
        drag.apply(&mut locator, Affine3A::IDENTITY, &camera, end, end - start);
        let turned = Quat::from_array(locator.rotation);
        let expected = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        assert!(turned.angle_between(expected) < 1e-3, "{turned:?}");
    }

    #[test]
    fn names_mirror_by_their_tokens() {
        let m = |n: &str| mirror_name(n);
        assert_eq!(m("hand_l").as_deref(), Some("hand_r"));
        assert_eq!(m("R.hand").as_deref(), Some("L.hand"));
        assert_eq!(
            m("b_LeftUpperArm_09").as_deref(),
            Some("b_RightUpperArm_09")
        );
        assert_eq!(m("foot-left").as_deref(), Some("foot-right"));
        assert_eq!(m("RIGHT EYE").as_deref(), Some("LEFT EYE"));
        assert_eq!(m("l_hand_r").as_deref(), Some("r_hand_l"));
        // Letters inside words are not tokens.
        assert_eq!(m("muzzle"), None);
        assert_eq!(m("hr_l2"), None);
    }

    #[test]
    fn joints_mirror_numbered_apart() {
        let joints: Vec<String> = ["b_LeftForeArm_010", "b_RightForeArm_07", "hand_l", "hand_r"]
            .map(String::from)
            .into();
        let m = |j: &str| mirror_joint(j, &joints);
        assert_eq!(m("b_LeftForeArm_010").as_deref(), Some("b_RightForeArm_07"));
        assert_eq!(m("b_RightForeArm_07").as_deref(), Some("b_LeftForeArm_010"));
        assert_eq!(m("hand_l").as_deref(), Some("hand_r"));
        assert_eq!(m("spine"), None);
        assert_eq!(m("foot_l"), None);
    }

    #[test]
    fn mirroring_reflects_without_mirroring() {
        let mut t = Locator::new("a".into());
        t.translation = [1.0, 2.0, 3.0];
        t.rotation = Quat::from_rotation_y(0.5).to_array();
        let world = local(&t);
        let m = mirrored(Affine3A::IDENTITY, world, 0);
        let (scale, rotation, translation) = m.to_scale_rotation_translation();
        assert!(translation.abs_diff_eq(Vec3::new(-1.0, 2.0, 3.0), 1e-5));
        assert!(scale.abs_diff_eq(Vec3::ONE, 1e-5), "not mirrored: {scale}");
        assert!(rotation.angle_between(Quat::from_rotation_y(-0.5)) < 1e-4);
        // Twice is back.
        let back = mirrored(Affine3A::IDENTITY, m, 0);
        assert!(back.abs_diff_eq(world, 1e-5));
    }

    #[test]
    fn rays_go_through_the_pointer() {
        let camera = test_camera();
        let (origin, dir) = camera.ray(pos2(400.0, 300.0));
        assert!(origin.abs_diff_eq(Vec3::Z * 4.0, 1e-4));
        assert!(dir.abs_diff_eq(Vec3::NEG_Z, 1e-4));
        let p = Vec3::new(0.3, -0.2, 0.5);
        let (origin, dir) = camera.ray(camera.project(p).unwrap());
        assert!((p - origin).normalize().abs_diff_eq(dir, 1e-4));
    }

    #[test]
    fn placing_turns_y_along_the_normal() {
        let mut locator = Locator::new("a".into());
        locator.scale = [2.0; 3];
        let parent = Affine3A::from_translation(Vec3::X);
        place(&mut locator, parent, Vec3::new(1.0, 1.0, 0.0), Vec3::X);
        assert!(Vec3::from(locator.translation).abs_diff_eq(Vec3::Y, 1e-5));
        let y = Quat::from_array(locator.rotation) * Vec3::Y;
        assert!(y.abs_diff_eq(Vec3::X, 1e-5));
        assert_eq!(locator.scale, [2.0; 3]);
    }

    #[test]
    fn local_transforms_round_trip() {
        let mut locator = Locator::new("a".into());
        locator.translation = [1.0, 2.0, 3.0];
        locator.rotation = Quat::from_rotation_y(0.5).to_array();
        locator.scale = [2.0, 2.0, 2.0];
        let mut copy = Locator::new("a".into());
        set_local(&mut copy, local(&locator));
        for (a, b) in copy.translation.iter().zip(locator.translation) {
            assert!(near(*a, b));
        }
        assert!(
            Quat::from_array(copy.rotation).angle_between(Quat::from_array(locator.rotation))
                < 1e-3
        );
        // A zero rotation counts as none.
        locator.rotation = [0.0; 4];
        assert_eq!(
            local(&locator).to_scale_rotation_translation().1,
            Quat::IDENTITY
        );
    }
}
