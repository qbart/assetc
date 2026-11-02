# `.aanim` animations

Every animation clip of a skinned glTF model, driving the joints of its
[`.amesh`](amesh.md) skeleton; see
[`docs/features/models.md`](../features/models.md#build). A
[chunked file](chunks.md) with format name `AANIM`, version `1`. The content
follows assetc-cpp's `.hanim` v1, laid out as plain arrays.

## Chunks

| Name       | Required | Payload                                                     |
|------------|----------|-------------------------------------------------------------|
| `DESC`     | yes      | [`AnimDesc`](#animdesc-32-bytes)                            |
| `CLIPS`    | yes      | [`Clip`](#clip-24-bytes)`[clip_count]`                      |
| `CHANNELS` | yes      | [`Channel`](#channel-24-bytes)`[channel_count]`             |
| `TIMES`    | yes      | `f32[key_count]`: key times in seconds, all channels        |
| `VALUES`   | yes      | `f32[value_count]`: key values, `components` per key        |
| `NAMES`    | yes      | UTF-8 clip names, back to back (not terminated)             |
| `EVENTS`   | no       | [`Event`](#event-24-bytes)`[event_count]`, see [Events](#events) |
| `EVNAMES`  | no       | UTF-8 event names, back to back (not terminated)            |

## AnimDesc (32 bytes)

| Offset | Size | Field           | Notes                                             |
|-------:|-----:|-----------------|---------------------------------------------------|
|      0 |    4 | `clip_count`    | u32                                               |
|      4 |    4 | `channel_count` | u32                                               |
|      8 |    4 | `key_count`     | u32, entries in `TIMES`                           |
|     12 |    4 | `value_count`   | u32, entries in `VALUES`                          |
|     16 |    4 | `names_size`    | u32, bytes in `NAMES`                             |
|     20 |    4 | `joint_count`   | u32, joints in the companion `.amesh` skeleton    |
|     24 |    8 | `mesh`          | u64, asset ID of the companion `.amesh`           |

## Clip (24 bytes)

| Offset | Size | Field           | Notes                                                 |
|-------:|-----:|-----------------|-------------------------------------------------------|
|      0 |    4 | `name_offset`   | u32, into `NAMES`                                     |
|      4 |    4 | `name_size`     | u32, bytes; the glTF name, or `Animation <n>` (from 1) without one |
|      8 |    4 | `duration`      | f32, seconds: the largest key time of the clip        |
|     12 |    4 | `first_channel` | u32, into `CHANNELS`                                  |
|     16 |    4 | `channel_count` | u32                                                   |
|     20 |    4 | `reserved`      | u32, `0`                                              |

Clips are in glTF file order.

## Channel (24 bytes)

| Offset | Size | Field           | Notes                                                    |
|-------:|-----:|-----------------|----------------------------------------------------------|
|      0 |    4 | `joint`         | u32, index into the `.amesh` `SKELETON`                  |
|      4 |    1 | `path`          | u8: `0` translation, `1` rotation, `2` scale             |
|      5 |    1 | `interpolation` | u8: `0` step, `1` linear                                 |
|      6 |    1 | `components`    | u8: `3` (translation, scale) or `4` (rotation `x y z w`) |
|      7 |    1 | `reserved`      | u8, `0`                                                  |
|      8 |    4 | `first_key`     | u32, into `TIMES`                                        |
|     12 |    4 | `key_count`     | u32                                                      |
|     16 |    4 | `first_value`   | u32, into `VALUES`; `key_count × components` floats       |
|     20 |    4 | `reserved`      | u32, `0`                                                 |

Key times increase within a channel. A joint's channels replace its bind-pose
translation, rotation or scale; the others keep the bind pose.

## Events

Named moments of the clips, see
[`docs/features/animation-events.md`](../features/animation-events.md).
`EVENTS` and `EVNAMES` come together, when the model has events;
`event_count` is `EVENTS`' size / 24. Events are sorted by `clip`, then
`time`, so a clip's events are one run (find it by binary search on
`clip`).

### Event (24 bytes)

| Offset | Size | Field         | Notes                                           |
|-------:|-----:|---------------|-------------------------------------------------|
|      0 |    4 | `clip`        | u32, index into `CLIPS`                         |
|      4 |    4 | `time`        | f32, seconds from the clip's start, 0 to its `duration` |
|      8 |    8 | `name_hash`   | u64, FNV-1a-64 of the name                      |
|     16 |    4 | `name_offset` | u32, into `EVNAMES`                             |
|     20 |    4 | `name_size`   | u32, bytes                                      |

## What is kept

- Only channels whose target node is a joint of the model's first skin.
  Channels on other nodes and morph-target `weights` channels are dropped.
- glTF `CUBICSPLINE` samplers become linear: of each in-tangent / value /
  out-tangent triple only the value is kept.
- Rotations are written as stored (normalized `f32` quaternions after
  reading normalized integer formats).
