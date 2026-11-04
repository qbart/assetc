//! `.aanim.bin` animation clips; see docs/spec/aanim.bin.md.

use crate::Result;
use crate::chunks::ChunkFile;
use crate::mesh::{Joint, run};
use crate::property::{Property, properties_of};
use crate::read::Bytes;

const FILE: &str = "aanim.bin";

/// What a channel animates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelPath {
    /// 3 components.
    Translation,
    /// 4 components, unit quaternion `x y z w`.
    Rotation,
    /// 3 components.
    Scale,
}

impl ChannelPath {
    /// Floats per key: 3, or 4 for a rotation.
    pub fn components(self) -> usize {
        match self {
            Self::Rotation => 4,
            _ => 3,
        }
    }

    /// The value that changes nothing: zero translation, identity rotation, unit scale.
    pub fn identity(self) -> [f32; 4] {
        match self {
            Self::Translation => [0.0; 4],
            Self::Rotation => [0.0, 0.0, 0.0, 1.0],
            Self::Scale => [1.0, 1.0, 1.0, 0.0],
        }
    }
}

/// How values between keys are found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    /// The key at or before the time.
    Step,
    /// Linear; spherical linear for rotations.
    Linear,
}

/// The keys of one joint's translation, rotation or scale; see
/// docs/spec/aanim.bin.md#channel-24-bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct Channel {
    /// Index into the `.amesh.bin` skeleton.
    pub joint: usize,
    pub path: ChannelPath,
    pub interpolation: Interpolation,
    /// Key times in seconds, increasing.
    pub times: Vec<f32>,
    /// `path.components()` floats per key.
    pub values: Vec<f32>,
}

impl Channel {
    /// The value at `time` (seconds), clamped to the first and last keys. Translation and
    /// scale fill `x y z` (the fourth is `0`), rotation is a unit quaternion `x y z w`. A
    /// channel without keys gives [`ChannelPath::identity`]. Does not allocate.
    pub fn sample(&self, time: f32) -> [f32; 4] {
        let n = self.path.components();
        let keys = self.times.len().min(self.values.len() / n);
        let key = |k: usize| {
            let mut out = self.path.identity();
            if let Some(v) = self.values.get(k * n..k * n + n) {
                out[..n].copy_from_slice(v);
            }
            if self.path == ChannelPath::Rotation {
                out = normalize(out);
            }
            out
        };
        if keys == 0 {
            return self.path.identity();
        }
        let times = self.times.get(..keys).unwrap_or_default();
        // The first key after `time`.
        let after = times.partition_point(|&t| t <= time);
        if after == 0 {
            return key(0);
        }
        if after >= keys || self.interpolation == Interpolation::Step {
            return key(after - 1);
        }
        let (t0, t1) = (times[after - 1], times[after]);
        let f = if t1 > t0 {
            (time - t0) / (t1 - t0)
        } else {
            0.0
        };
        let (a, b) = (key(after - 1), key(after));
        if self.path == ChannelPath::Rotation {
            slerp(a, b, f)
        } else {
            std::array::from_fn(|c| a[c] + (b[c] - a[c]) * f)
        }
    }
}

/// A joint's local transform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub translation: [f32; 3],
    /// Unit quaternion `x y z w`.
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        }
    }
}

impl From<&Joint> for Transform {
    /// The joint's bind pose.
    fn from(joint: &Joint) -> Self {
        Self {
            translation: joint.bind_translation,
            rotation: joint.bind_rotation,
            scale: joint.bind_scale,
        }
    }
}

/// One animation; see docs/spec/aanim.bin.md#clip-24-bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    /// The glTF name, or `Animation <n>`.
    pub name: String,
    /// Seconds: the largest key time.
    pub duration: f32,
    pub channels: Vec<Channel>,
}

impl Clip {
    /// Poses `joints` (local transforms indexed like the skeleton, usually starting from
    /// the bind pose) at `time`: each channel replaces its joint's translation, rotation or
    /// scale. Channels of joints past `joints` are skipped. Does not allocate.
    pub fn apply(&self, time: f32, joints: &mut [Transform]) {
        for channel in &self.channels {
            let Some(joint) = joints.get_mut(channel.joint) else {
                continue;
            };
            let [x, y, z, w] = channel.sample(time);
            match channel.path {
                ChannelPath::Translation => joint.translation = [x, y, z],
                ChannelPath::Rotation => joint.rotation = [x, y, z, w],
                ChannelPath::Scale => joint.scale = [x, y, z],
            }
        }
    }
}

/// A named moment of a clip; see docs/spec/aanim.bin.md#events.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// Index into [`Animation::clips`].
    pub clip: usize,
    /// Seconds from the clip's start.
    pub time: f32,
    /// FNV-1a-64 of the name.
    pub name_hash: u64,
    pub name: String,
    /// Sorted by key.
    pub properties: Vec<Property>,
}

/// The animation clips of a skinned mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    /// Joints in the companion `.amesh.bin` skeleton.
    pub joint_count: u32,
    /// Asset ID of the companion `.amesh.bin`.
    pub mesh: u64,
    pub clips: Vec<Clip>,
    /// Sorted by clip, then time.
    pub events: Vec<Event>,
}

impl Animation {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let file = ChunkFile::open(bytes, "AANIM", &[1], FILE)?;
        let require = |name| Ok::<_, crate::Error>(Bytes::new(file.require(name, FILE)?, FILE));
        let chunk = |name| Bytes::new(file.chunk(name).unwrap_or_default(), FILE);

        let desc = require("DESC")?;
        let clip_count = desc.u32(0)? as usize;
        let channel_count = desc.u32(4)? as usize;
        let key_count = desc.u32(8)? as usize;
        let value_count = desc.u32(12)? as usize;
        let names_size = desc.u32(16)? as usize;
        let joint_count = desc.u32(20)?;

        let times = require("TIMES")?
            .records_n(4, key_count)?
            .map(|t| t.f32(0))
            .collect::<Result<Vec<_>>>()?;
        let values = require("VALUES")?
            .records_n(4, value_count)?
            .map(|v| v.f32(0))
            .collect::<Result<Vec<_>>>()?;
        let channels = require("CHANNELS")?
            .records_n(24, channel_count)?
            .map(|c| {
                let path = match c.u8(4)? {
                    0 => ChannelPath::Translation,
                    1 => ChannelPath::Rotation,
                    2 => ChannelPath::Scale,
                    p => return Err(c.err(format!("unknown channel path {p}"))),
                };
                let interpolation = match c.u8(5)? {
                    0 => Interpolation::Step,
                    1 => Interpolation::Linear,
                    i => return Err(c.err(format!("unknown interpolation {i}"))),
                };
                let components = usize::from(c.u8(6)?);
                if components != path.components() {
                    return Err(c.err(format!("{components} components for {path:?}")));
                }
                let joint = c.u32(0)?;
                if joint >= joint_count {
                    return Err(c.err(format!("joint {joint} past {joint_count} joints")));
                }
                let keys = c.u32(12)? as usize;
                let value_len = keys
                    .checked_mul(components)
                    .ok_or_else(|| c.err("value count overflows"))?;
                Ok(Channel {
                    joint: joint as usize,
                    path,
                    interpolation,
                    times: run(&times, c.u32(8)? as usize, keys, &c)?.to_vec(),
                    values: run(&values, c.u32(16)? as usize, value_len, &c)?.to_vec(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let names = Bytes::new(require("NAMES")?.slice(0, names_size)?, FILE);
        let clips = require("CLIPS")?
            .records_n(24, clip_count)?
            .map(|c| {
                Ok(Clip {
                    name: c.text_ref(0, names)?,
                    duration: c.f32(8)?,
                    channels: run(&channels, c.u32(12)? as usize, c.u32(16)? as usize, &c)?
                        .to_vec(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let text = chunk("EVTEXT");
        let props = properties_of(chunk("EVPROPS"), text)?;
        let events = chunk("EVENTS")
            .records(32)?
            .map(|e| {
                let clip = e.u32(0)? as usize;
                if clip >= clips.len() {
                    return Err(e.err(format!("event clip {clip} past {} clips", clips.len())));
                }
                Ok(Event {
                    clip,
                    time: e.f32(4)?,
                    name_hash: e.u64(8)?,
                    name: e.text_ref(16, text)?,
                    properties: run(&props, e.u32(24)? as usize, e.u32(28)? as usize, &e)?.to_vec(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            joint_count,
            mesh: desc.u64(24)?,
            clips,
            events,
        })
    }

    /// The clip named `name`.
    pub fn clip(&self, name: &str) -> Option<&Clip> {
        self.clips.iter().find(|c| c.name == name)
    }

    /// The events of clip `clip`, by time.
    pub fn events_of(&self, clip: usize) -> &[Event] {
        let start = self.events.partition_point(|e| e.clip < clip);
        let end = self.events.partition_point(|e| e.clip <= clip);
        self.events.get(start..end).unwrap_or_default()
    }
}

fn normalize(q: [f32; 4]) -> [f32; 4] {
    let length = q.iter().map(|c| c * c).sum::<f32>().sqrt();
    if length > f32::EPSILON && length.is_finite() {
        q.map(|c| c / length)
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

/// Spherical linear interpolation of unit quaternions along the shorter arc.
fn slerp(a: [f32; 4], mut b: [f32; 4], f: f32) -> [f32; 4] {
    let mut dot: f32 = (0..4).map(|c| a[c] * b[c]).sum();
    if dot < 0.0 {
        b = b.map(|c| -c);
        dot = -dot;
    }
    let (wa, wb) = if dot > 0.9995 {
        // Nearly parallel: normalized linear is exact enough and stable.
        (1.0 - f, f)
    } else {
        let theta = dot.min(1.0).acos();
        let sin = theta.sin();
        (((1.0 - f) * theta).sin() / sin, (f * theta).sin() / sin)
    };
    normalize(std::array::from_fn(|c| a[c] * wa + b[c] * wb))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::tests::file;
    use crate::mesh::PropertyValue;
    use crate::mesh::tests::{Out, Strings, property};

    const HALF: f32 = std::f32::consts::FRAC_1_SQRT_2;

    /// A clip `Walk` of a translation, a rotation (0 to 90° about Z) and a step scale
    /// channel, with two events; `components` is the rotation's, `skip` leaves chunks out.
    fn anim(components: u8, skip: &[&str]) -> Vec<u8> {
        let mut desc = Out::default();
        desc.u32(1).u32(3).u32(6).u32(20).u32(4).u32(2).u64(0xF0C5);
        let mut clips = Out::default();
        clips.u32(0).u32(4).f32s(&[2.0]).u32(0).u32(3).u32(0);
        let mut channels = Out::default();
        // joint, path, interpolation, components, first key, keys, first value.
        for (joint, path, interpolation, n, key, value) in [
            (0, 0, 1, 3, 0, 0),
            (1, 1, 1, components, 2, 6),
            (1, 2, 0, 3, 4, 14),
        ] {
            channels
                .u32(joint)
                .u8(path)
                .u8(interpolation)
                .u8(n)
                .u8(0)
                .u32(key)
                .u32(2)
                .u32(value)
                .u32(0);
        }
        let mut times = Out::default();
        times.f32s(&[0.0, 1.0, 0.0, 1.0, 0.5, 2.0]);
        let mut values = Out::default();
        values
            .f32s(&[0.0, 0.0, 0.0, 2.0, 4.0, 6.0])
            .f32s(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, HALF, HALF])
            .f32s(&[1.0, 1.0, 1.0, 3.0, 3.0, 3.0]);
        let mut text = Strings::default();
        let mut events = Out::default();
        let mut props = Out::default();
        for (time, name, first, count) in [(0.25, "step", 0, 1), (1.5, "land", 0, 0)] {
            let (at, len) = text.add(name);
            events
                .u32(0)
                .f32s(&[time])
                .u64(crate::hash::fnv1a64(name.as_bytes()))
                .u32(at)
                .u32(len)
                .u32(first)
                .u32(count);
            if count > 0 {
                property(&mut props, &mut text, "foot", 0.0, Some("left"));
            }
        }
        let chunks: Vec<(&str, &[u8])> = vec![
            ("DESC", &desc.0),
            ("CLIPS", &clips.0),
            ("CHANNELS", &channels.0),
            ("TIMES", &times.0),
            ("VALUES", &values.0),
            ("NAMES", b"Walk"),
            ("EVENTS", &events.0),
            ("EVPROPS", &props.0),
            ("EVTEXT", &text.0),
        ];
        let chunks: Vec<_> = chunks
            .into_iter()
            .filter(|(n, _)| !skip.contains(n))
            .collect();
        file("AANIM", 1, &chunks)
    }

    fn close(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-5)
    }

    #[test]
    fn reads_clips_channels_and_events() {
        let a = Animation::decode(&anim(4, &[])).unwrap();
        assert_eq!((a.joint_count, a.mesh), (2, 0xF0C5));
        let walk = a.clip("Walk").unwrap();
        assert_eq!((walk.duration, walk.channels.len()), (2.0, 3));
        let rotation = &walk.channels[1];
        assert_eq!(rotation.joint, 1);
        assert_eq!(rotation.path, ChannelPath::Rotation);
        assert_eq!(rotation.interpolation, Interpolation::Linear);
        assert_eq!(rotation.times, [0.0, 1.0]);
        assert_eq!(rotation.values.len(), 8);
        assert_eq!(walk.channels[2].interpolation, Interpolation::Step);
        let events = a.events_of(0);
        assert_eq!(events.len(), 2);
        assert_eq!((events[0].name.as_str(), events[0].time), ("step", 0.25));
        assert_eq!(events[0].name_hash, crate::hash::fnv1a64(b"step"));
        assert_eq!(events[0].properties[0].key, "foot");
        assert_eq!(
            events[0].properties[0].value,
            PropertyValue::Text("left".into())
        );
        assert!(events[1].properties.is_empty());
        assert!(a.events_of(1).is_empty());
        let none = Animation::decode(&anim(4, &["EVENTS", "EVPROPS", "EVTEXT"])).unwrap();
        assert!(none.events.is_empty());
    }

    #[test]
    fn samples_between_and_outside_keys() {
        let a = Animation::decode(&anim(4, &[])).unwrap();
        let [translation, rotation, scale] = [0, 1, 2].map(|c| &a.clips[0].channels[c]);
        assert!(close(translation.sample(0.25), [0.5, 1.0, 1.5, 0.0]));
        assert!(close(translation.sample(-1.0), [0.0; 4]));
        assert!(close(translation.sample(9.0), [2.0, 4.0, 6.0, 0.0]));
        // Halfway from identity to 90° about Z is 45°.
        let (s, c) = (
            std::f32::consts::FRAC_PI_8.sin(),
            std::f32::consts::FRAC_PI_8.cos(),
        );
        assert!(close(rotation.sample(0.5), [0.0, 0.0, s, c]));
        assert!(close(rotation.sample(1.0), [0.0, 0.0, HALF, HALF]));
        assert!(close(rotation.sample(-0.5), [0.0, 0.0, 0.0, 1.0]));
        // Step holds the earlier key.
        assert!(close(scale.sample(0.0), [1.0, 1.0, 1.0, 0.0]));
        assert!(close(scale.sample(1.9), [1.0, 1.0, 1.0, 0.0]));
        assert!(close(scale.sample(2.0), [3.0, 3.0, 3.0, 0.0]));
        let empty = Channel {
            joint: 0,
            path: ChannelPath::Rotation,
            interpolation: Interpolation::Linear,
            times: Vec::new(),
            values: Vec::new(),
        };
        assert_eq!(empty.sample(1.0), [0.0, 0.0, 0.0, 1.0]);

        let mut pose = [Transform::default(); 2];
        a.clips[0].apply(1.0, &mut pose);
        assert_eq!(pose[0].translation, [2.0, 4.0, 6.0]);
        assert!(close(pose[1].rotation, [0.0, 0.0, HALF, HALF]));
        assert_eq!(pose[1].scale, [1.0; 3]);
        a.clips[0].apply(1.0, &mut pose[..1]);
    }

    #[test]
    fn rejects_bad_files() {
        assert!(Animation::decode(&anim(4, &["NAMES"])).is_err());
        assert!(Animation::decode(&anim(4, &["EVTEXT"])).is_err());
        assert!(Animation::decode(&anim(3, &[])).is_err());
        let bytes = anim(4, &[]);
        assert!(Animation::decode(&bytes[..bytes.len() - 4]).is_err());
    }
}
