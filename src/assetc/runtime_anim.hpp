#pragma once

#include "runtime_mesh.hpp" // MakeFourCC

#include <cstdint>
#include <string>
#include <vector>

namespace assetc
{

constexpr uint32_t AnimMagic   = MakeFourCC('H', 'A', 'N', 'M');
constexpr uint32_t AnimVersion = 1;

// Animation target property. Matches glTF channel target paths (weights/morph
// targets are not exported).
enum class AnimPath : uint8_t
{
    Translation = 0, // vec3
    Rotation    = 1, // vec4 quaternion (x,y,z,w)
    Scale       = 2, // vec3
};

enum class AnimInterp : uint8_t
{
    Step   = 0,
    Linear = 1,
};

struct AnimKey
{
    float time;
    float value[4]; // vec3 uses [0..2]; rotation uses all 4
};

// One animated joint property over time. `joint` indexes into the mesh SKEL array.
struct AnimChannel
{
    uint32_t             joint;
    AnimPath             path;
    AnimInterp           interp;
    std::vector<AnimKey> keys; // sorted by time
};

struct AnimClip
{
    std::string              name;
    float                    duration = 0.0f; // seconds (max key time)
    std::vector<AnimChannel> channels;
};

// `.hanim` companion file ('HANM'): all animation clips for one source.
//   magic u32, version u32, clipCount u32, reserved u32
//   per clip: nameLen u16, name, duration f32, channelCount u32
//     per channel: joint u32, path u8, interp u8, components u8 (3|4), _pad u8,
//                  keyCount u32, keys[ time f32, value[components] f32 ]
int WriteHAnim(const std::string &path, const std::vector<AnimClip> &clips);

// Validate magic/version and that the file parses with exact byte consumption.
int ValidateHAnim(const std::string &path);

// Parse a `.hanim` into `out` (cleared first). 0 on success.
int ReadHAnim(const std::string &path, std::vector<AnimClip> &out);

} // namespace assetc
