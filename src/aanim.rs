//! `.aanim` animation clips of a skinned mesh, see docs/spec/aanim.md.

use crate::chunks::{self, Bytes, Writer};

pub const FORMAT: &str = "AANIM";
pub const VERSION: u32 = 1;

pub struct Clip {
    pub name: String,
    pub channels: Vec<Channel>,
}

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
        let (mut records, mut names) = (Bytes::default(), Vec::new());
        for event in sorted {
            records
                .u32(event.clip)
                .f32s(&[event.time])
                .u64(crate::hash::fnv1a64(event.name.as_bytes()))
                .u32(names.len() as u32)
                .u32(event.name.len() as u32);
            names.extend_from_slice(event.name.as_bytes());
        }
        writer
            .chunk("EVENTS", records.take())
            .chunk("EVNAMES", names);
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
    let events = reader.chunk("EVENTS").map_or(0, |e| e.len() / 24);
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
        };
        let events = [
            event(1, 1.0, "hit"),
            event(0, 0.5, "step"),
            event(0, 0.25, "step"),
        ];
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

        // Events by clip, then time.
        let records = reader.require("EVENTS").unwrap();
        assert_eq!(records.len(), 3 * 24);
        assert_eq!(chunks::f32_at(records, 0, 24, 4), Some(0.25));
        assert_eq!(chunks::u32_at(records, 2, 24, 0), Some(1));
        assert_eq!(chunks::u32_at(records, 2, 24, 16), Some(8));
        assert_eq!(records[8..16], crate::hash::fnv1a64(b"step").to_le_bytes());
        assert_eq!(reader.require("EVNAMES").unwrap(), b"stepstephit");

        let rows = rows(&bytes).unwrap();
        assert_eq!(rows[0], ("Clips", "Walk (0.75 s), Run (1.25 s)".into()));
        assert_eq!(rows[3], ("Events", "3".into()));
        let plain = encode(&[clip("Walk", 0.75)], &[], 2, 7);
        let reader = chunks::Reader::parse(&plain, FORMAT).unwrap();
        assert!(reader.chunk("EVENTS").is_none());
    }
}
