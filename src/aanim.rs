//! `.aanim` animation clips of a skinned mesh, see docs/spec/aanim.md.

use crate::chunks::{self, Bytes, Writer};

pub const FORMAT: &str = "AANIM";
pub const VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct Clip {
    pub name: String,
    pub channels: Vec<Channel>,
}

#[derive(Debug, Clone)]
pub struct Channel {
    pub joint: u32,
    /// 0 translation, 1 rotation, 2 scale.
    pub path: u8,
    /// 0 step, 1 linear.
    pub interpolation: u8,
    pub components: u8,
    pub times: Vec<f32>,
    /// `times.len() × components`.
    pub values: Vec<f32>,
}

/// A named moment of a clip; see docs/spec/aanim.md#events.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// Index into the clips.
    pub clip: u32,
    pub time: f32,
    pub name: String,
    pub properties: crate::enrich::Properties,
}

impl Clip {
    pub fn duration(&self) -> f32 {
        self.channels
            .iter()
            .flat_map(|c| c.times.iter().copied())
            .fold(0.0, f32::max)
    }
}

/// Encodes the clips and their events for a skeleton of `joint_count` joints in mesh `mesh`
/// (its asset ID).
pub fn encode(clips: &[Clip], events: &[Event], joint_count: u32, mesh: u64) -> Vec<u8> {
    let (mut table, mut channels, mut times, mut values, mut names) = (
        Bytes::default(),
        Bytes::default(),
        Bytes::default(),
        Bytes::default(),
        Vec::new(),
    );
    let (mut channel_count, mut key_count, mut value_count) = (0u32, 0u32, 0u32);
    for clip in clips {
        table
            .u32(names.len() as u32)
            .u32(clip.name.len() as u32)
            .f32s(&[clip.duration()])
            .u32(channel_count)
            .u32(clip.channels.len() as u32)
            .u32(0);
        names.extend_from_slice(clip.name.as_bytes());
        for c in &clip.channels {
            channels
                .u32(c.joint)
                .u8(c.path)
                .u8(c.interpolation)
                .u8(c.components)
                .u8(0)
                .u32(key_count)
                .u32(c.times.len() as u32)
                .u32(value_count)
                .u32(0);
            times.f32s(&c.times);
            values.f32s(&c.values);
            key_count += c.times.len() as u32;
            value_count += c.values.len() as u32;
        }
        channel_count += clip.channels.len() as u32;
    }
    let mut desc = Bytes::default();
    desc.u32(clips.len() as u32)
        .u32(channel_count)
        .u32(key_count)
        .u32(value_count)
        .u32(names.len() as u32)
        .u32(joint_count)
        .u64(mesh);
    let mut writer = Writer::new(FORMAT, VERSION);
    writer
        .chunk("DESC", desc.take())
        .chunk("CLIPS", table.take())
        .chunk("CHANNELS", channels.take())
        .chunk("TIMES", times.take())
        .chunk("VALUES", values.take())
        .chunk("NAMES", names);
    if !events.is_empty() {
        // By clip, then time; ties keep their order.
        let mut sorted: Vec<&Event> = events.iter().collect();
        sorted.sort_by(|a, b| a.clip.cmp(&b.clip).then(a.time.total_cmp(&b.time)));
        let (mut records, mut properties) = (Bytes::default(), Bytes::default());
        let mut text = crate::amesh::Strings(Vec::new());
        let mut property_count = 0;
        for event in sorted {
            let (at, len) = text.add(&event.name);
            records
                .u32(event.clip)
                .f32s(&[event.time])
                .u64(crate::hash::fnv1a64(event.name.as_bytes()))
                .u32(at)
                .u32(len)
                .u32(property_count)
                .u32(event.properties.len() as u32);
            crate::amesh::put_properties(&mut properties, &mut text, &event.properties);
            property_count += event.properties.len() as u32;
        }
        writer.chunk("EVENTS", records.take());
        if property_count > 0 {
            writer.chunk("EVPROPS", properties.take());
        }
        writer.chunk("EVTEXT", text.0);
    }
    writer.finish()
}

/// Inspector rows of an `.aanim` file; see docs/features/models.md#inspector.
pub fn rows(bytes: &[u8]) -> Result<Vec<(&'static str, String)>, String> {
    let reader = chunks::Reader::parse(bytes, FORMAT)?;
    if reader.version != VERSION {
        return Err(format!("unsupported {FORMAT} version {}", reader.version));
    }
    let desc = reader.require("DESC")?;
    let field = |at| chunks::u32_at(desc, 0, 32, at).ok_or("DESC is too short");
    let (clips, names) = (reader.require("CLIPS")?, reader.require("NAMES")?);
    let listed: Vec<String> = (0..field(0)? as usize)
        .map(|i| {
            let offset = chunks::u32_at(clips, i, 24, 0).unwrap_or(0) as usize;
            let size = chunks::u32_at(clips, i, 24, 4).unwrap_or(0) as usize;
            let name = names
                .get(offset..offset + size)
                .map(String::from_utf8_lossy)
                .unwrap_or_default();
            let duration = chunks::f32_at(clips, i, 24, 8).unwrap_or(0.0);
            format!("{name} ({duration:.2} s)")
        })
        .collect();
    let events = reader.chunk("EVENTS").map_or(0, |e| e.len() / 32);
    Ok(vec![
        ("Clips", listed.join(", ")),
        ("Channels", field(4)?.to_string()),
        ("Keys", field(8)?.to_string()),
        (
            "Events",
            if events > 0 {
                events.to_string()
            } else {
                "none".into()
            },
        ),
    ])
}

/// An `.aanim` read back: its clips and events, and its skeleton's joint count.
#[derive(Debug, Clone)]
pub struct Decoded {
    pub clips: Vec<Clip>,
    pub events: Vec<Event>,
    pub joint_count: u32,
}

/// Reads an `.aanim` back; see docs/features/models.md#compiled-animation-preview.
pub fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    use crate::enrich::PropValue;
    let reader = chunks::Reader::parse(bytes, FORMAT)?;
    if reader.version != VERSION {
        return Err(format!("unsupported {FORMAT} version {}", reader.version));
    }
    let bad = || "animation data out of range".to_owned();
    let desc = reader.require("DESC")?;
    let field = |at| chunks::u32_at(desc, 0, 32, at).ok_or_else(bad);
    let (clip_count, joint_count) = (field(0)? as usize, field(20)?);
    let (table, channels) = (reader.require("CLIPS")?, reader.require("CHANNELS")?);
    let (times, values, names) = (
        reader.require("TIMES")?,
        reader.require("VALUES")?,
        reader.require("NAMES")?,
    );
    let floats = |p: &[u8], first: usize, count: usize| -> Option<Vec<f32>> {
        (first..first + count)
            .map(|i| chunks::f32_at(p, i, 4, 0))
            .collect()
    };
    let text = |p: &[u8], at: usize, len: usize| {
        p.get(at..at + len)
            .map(|t| String::from_utf8_lossy(t).into_owned())
    };
    let mut clips = Vec::with_capacity(clip_count);
    for c in 0..clip_count {
        let u = |at| {
            chunks::u32_at(table, c, 24, at)
                .map(|v| v as usize)
                .ok_or_else(bad)
        };
        let name = text(names, u(0)?, u(4)?).ok_or_else(bad)?;
        let (first, count) = (u(12)?, u(16)?);
        let mut list = Vec::with_capacity(count);
        for k in first..first + count {
            let u = |at| chunks::u32_at(channels, k, 24, at).ok_or_else(bad);
            let byte = |at: usize| channels.get(k * 24 + at).copied().ok_or_else(bad);
            let components = byte(6)?;
            let (first_key, key_count) = (u(8)? as usize, u(12)? as usize);
            let first_value = u(16)? as usize;
            list.push(Channel {
                joint: u(0)?,
                path: byte(4)?,
                interpolation: byte(5)?,
                components,
                times: floats(times, first_key, key_count).ok_or_else(bad)?,
                values: floats(values, first_value, key_count * components as usize)
                    .ok_or_else(bad)?,
            });
        }
        clips.push(Clip {
            name,
            channels: list,
        });
    }
    let mut events = Vec::new();
    if let Some(records) = reader.chunk("EVENTS") {
        let evtext = reader.chunk("EVTEXT").unwrap_or_default();
        let props = reader.chunk("EVPROPS").unwrap_or_default();
        for e in 0..records.len() / 32 {
            let u = |at| {
                chunks::u32_at(records, e, 32, at)
                    .map(|v| v as usize)
                    .ok_or_else(bad)
            };
            let name = text(evtext, u(16)?, u(20)?).ok_or_else(bad)?;
            let mut properties = crate::enrich::Properties::new();
            for p in u(24)?..u(24)? + u(28)? {
                let pu = |at| {
                    chunks::u32_at(props, p, 32, at)
                        .map(|v| v as usize)
                        .ok_or_else(bad)
                };
                let key = text(evtext, pu(8)?, pu(12)?).ok_or_else(bad)?;
                let raw: [u8; 8] = props
                    .get(p * 32 + 24..p * 32 + 32)
                    .ok_or_else(bad)?
                    .try_into()
                    .unwrap();
                let value = match pu(16)? {
                    0 => PropValue::Bool(u64::from_le_bytes(raw) != 0),
                    1 => PropValue::Int(i64::from_le_bytes(raw)),
                    2 => PropValue::Float(f64::from_le_bytes(raw)),
                    _ => PropValue::Text(text(evtext, pu(24)?, pu(28)?).ok_or_else(bad)?),
                };
                properties.insert(key, value);
            }
            events.push(Event {
                clip: u(0)? as u32,
                time: chunks::f32_at(records, e, 32, 4).ok_or_else(bad)?,
                name,
                properties,
            });
        }
    }
    Ok(Decoded {
        clips,
        events,
        joint_count,
    })
}

/// A joint's local transform: translation, rotation (`x y z w`) and scale.
pub type Local = ([f32; 3], [f32; 4], [f32; 3]);

/// The clip's joint transforms at `time`, from `rest` (the bind pose) for joints its
/// channels leave alone; see docs/features/models.md#compiled-animation-preview.
pub fn sample(clip: &Clip, time: f32, rest: &[Local]) -> Vec<Local> {
    use bevy::math::{Quat, Vec3, Vec4};
    let mut out = rest.to_vec();
    for channel in &clip.channels {
        let Some(local) = out.get_mut(channel.joint as usize) else {
            continue;
        };
        let n = channel.components as usize;
        let keys = channel.times.len();
        if keys == 0 || channel.values.len() < keys * n {
            continue;
        }
        let value = |k: usize| &channel.values[k * n..k * n + n];
        // The key at or before `time`, and how far to the next.
        let after = channel.times.partition_point(|&t| t <= time);
        let (a, b, f) = match after {
            0 => (0, 0, 0.0),
            k if k >= keys => (keys - 1, keys - 1, 0.0),
            k => {
                let (t0, t1) = (channel.times[k - 1], channel.times[k]);
                let f = if channel.interpolation == 0 || t1 <= t0 {
                    0.0
                } else {
                    (time - t0) / (t1 - t0)
                };
                (k - 1, k, f)
            }
        };
        match (channel.path, n) {
            (1, 4) => {
                let q = |v: &[f32]| Quat::from_vec4(Vec4::new(v[0], v[1], v[2], v[3])).normalize();
                local.1 = q(value(a)).slerp(q(value(b)), f).to_array();
            }
            (path, 3) => {
                let v = |v: &[f32]| Vec3::new(v[0], v[1], v[2]);
                let at = v(value(a)).lerp(v(value(b)), f).to_array();
                if path == 0 {
                    local.0 = at;
                } else {
                    local.2 = at;
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_clips_as_arrays() {
        let clip = |name: &str, end: f32| Clip {
            name: name.into(),
            channels: vec![
                Channel {
                    joint: 1,
                    path: 1,
                    interpolation: 1,
                    components: 4,
                    times: vec![0.0, end],
                    values: vec![0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0],
                },
                Channel {
                    joint: 0,
                    path: 0,
                    interpolation: 0,
                    components: 3,
                    times: vec![0.5],
                    values: vec![1.0, 2.0, 3.0],
                },
            ],
        };
        let event = |clip, time, name: &str| Event {
            clip,
            time,
            name: name.into(),
            properties: Default::default(),
        };
        let mut hit = event(1, 1.0, "hit");
        hit.properties
            .insert("damage".into(), crate::enrich::PropValue::Int(3));
        let events = [hit, event(0, 0.5, "step"), event(0, 0.25, "step")];
        let bytes = encode(&[clip("Walk", 0.75), clip("Run", 1.25)], &events, 2, 7);
        let reader = chunks::Reader::parse(&bytes, FORMAT).unwrap();
        let desc = reader.require("DESC").unwrap();
        assert_eq!(chunks::u32_at(desc, 0, 32, 4), Some(4));
        assert_eq!(chunks::u32_at(desc, 0, 32, 8), Some(6));
        assert_eq!(chunks::u32_at(desc, 0, 32, 12), Some(22));
        assert_eq!(desc[24..32], 7u64.to_le_bytes());
        let channels = reader.require("CHANNELS").unwrap();
        assert_eq!(channels.len(), 4 * 24);
        // The second clip's first channel starts after the first clip's keys and values.
        assert_eq!(chunks::u32_at(channels, 2, 24, 8), Some(3));
        assert_eq!(chunks::u32_at(channels, 2, 24, 16), Some(11));
        assert_eq!(reader.require("TIMES").unwrap().len(), 6 * 4);
        assert_eq!(reader.require("NAMES").unwrap(), b"WalkRun");

        // Read back whole.
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.joint_count, 2);
        assert_eq!(decoded.clips[1].name, "Run");
        assert_eq!(decoded.clips[1].channels[0].values.len(), 8);
        assert_eq!(decoded.events.len(), 3);
        assert_eq!(
            decoded.events[2].properties["damage"],
            crate::enrich::PropValue::Int(3)
        );
        // Sampling: halfway through Walk's rotation, the translation key held.
        let rest = vec![([0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0; 3]); 2];
        let pose = sample(&decoded.clips[0], 0.375, &rest);
        assert_eq!(pose[0].0, [1.0, 2.0, 3.0]);
        let half = (0.5f32).sqrt();
        assert!((pose[1].1[1] - half).abs() < 1e-4 && (pose[1].1[3] - half).abs() < 1e-4);
        // Events by clip, then time.
        let records = reader.require("EVENTS").unwrap();
        assert_eq!(records.len(), 3 * 32);
        assert_eq!(chunks::f32_at(records, 0, 32, 4), Some(0.25));
        assert_eq!(chunks::u32_at(records, 2, 32, 0), Some(1));
        assert_eq!(chunks::u32_at(records, 2, 32, 16), Some(8));
        assert_eq!(records[8..16], crate::hash::fnv1a64(b"step").to_le_bytes());
        // The hit's one property: first 0, count 1; its key after its name.
        assert_eq!(chunks::u32_at(records, 2, 32, 24), Some(0));
        assert_eq!(chunks::u32_at(records, 2, 32, 28), Some(1));
        let props = reader.require("EVPROPS").unwrap();
        assert_eq!(props.len(), 32);
        assert_eq!(chunks::u32_at(props, 0, 32, 8), Some(11));
        assert_eq!(props[24..32], 3i64.to_le_bytes());
        assert_eq!(reader.require("EVTEXT").unwrap(), b"stepstephitdamage");

        let rows = rows(&bytes).unwrap();
        assert_eq!(rows[0], ("Clips", "Walk (0.75 s), Run (1.25 s)".into()));
        assert_eq!(rows[3], ("Events", "3".into()));
        let plain = encode(&[clip("Walk", 0.75)], &[], 2, 7);
        let reader = chunks::Reader::parse(&plain, FORMAT).unwrap();
        assert!(reader.chunk("EVENTS").is_none());
    }
}
