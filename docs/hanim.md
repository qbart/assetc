# .hanim file format (v1)

Little-endian. Magic `"HANM"`. A skinned, animated glTF emits a companion `.hanim` (`runtime/<name>.hanim`) alongside its [`.hmesh`](hmesh.md), holding every animation clip for that source. Channels index joints in the companion `.hmesh` `SKEL` array.

```
magic u32 'HANM' | version u32 1 | clipCount u32 | reserved u32
per clip:
    nameLen u16, name bytes
    duration f32                      // seconds (max key time)
    channelCount u32
    per channel:
        joint u32                     // index into SKEL
        path u8        (0=translation vec3, 1=rotation vec4 xyzw, 2=scale vec3)
        interp u8      (0=STEP, 1=LINEAR)
        components u8  (3 or 4)
        _pad u8
        keyCount u32
        per key: time f32, value[components] f32
```

Morph-target (`weights`) channels and channels targeting non-joint nodes are skipped. `CUBICSPLINE` samplers are degraded to `LINEAR` (the value keyframe of each tangent triple is kept) with a warning. `assetc info` lists clips/channels/duration; `assetc check` validates that every channel's joint index is within the companion skeleton.
