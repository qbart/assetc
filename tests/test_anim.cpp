#include "check.hpp"

#include "assetc/runtime_anim.hpp"

#include <filesystem>
#include <fstream>
#include <iterator>
#include <string>
#include <vector>

namespace fs = std::filesystem;
using namespace assetc;

int main()
{
    const auto dir = fs::temp_directory_path() / "assetc_test_anim";
    fs::create_directories(dir);
    const auto path = (dir / "a.hanim").string();

    std::vector<AnimClip> clips;
    {
        AnimClip clip;
        clip.name     = "Walk";
        clip.duration = 2.0f;

        AnimChannel t;
        t.joint  = 3;
        t.path   = AnimPath::Translation;
        t.interp = AnimInterp::Linear;
        t.keys   = {{0.0f, {1, 2, 3, 0}}, {1.0f, {4, 5, 6, 0}}, {2.0f, {7, 8, 9, 0}}};
        clip.channels.push_back(t);

        AnimChannel r;
        r.joint  = 7;
        r.path   = AnimPath::Rotation;
        r.interp = AnimInterp::Step;
        r.keys   = {{0.0f, {0, 0, 0, 1}}, {2.0f, {0, 0.707f, 0, 0.707f}}};
        clip.channels.push_back(r);

        clips.push_back(clip);
    }
    {
        AnimClip clip;
        clip.name     = "Idle";
        clip.duration = 0.5f;
        AnimChannel s;
        s.joint  = 0;
        s.path   = AnimPath::Scale;
        s.interp = AnimInterp::Linear;
        s.keys   = {{0.0f, {1, 1, 1, 0}}};
        clip.channels.push_back(s);
        clips.push_back(clip);
    }

    CHECK_EQ(WriteHAnim(path, clips), 0);
    CHECK_EQ(ValidateHAnim(path), 0);

    std::vector<AnimClip> back;
    CHECK_EQ(ReadHAnim(path, back), 0);
    CHECK_EQ(back.size(), 2u);
    CHECK_EQ(back[0].name, std::string("Walk"));
    CHECK_NEAR(back[0].duration, 2.0f, 1e-6);
    CHECK_EQ(back[0].channels.size(), 2u);

    const auto &t = back[0].channels[0];
    CHECK_EQ(t.joint, 3u);
    CHECK(t.path == AnimPath::Translation);
    CHECK(t.interp == AnimInterp::Linear);
    CHECK_EQ(t.keys.size(), 3u);
    CHECK_NEAR(t.keys[2].time, 2.0f, 1e-6);
    CHECK_NEAR(t.keys[2].value[0], 7.0f, 1e-6);
    CHECK_NEAR(t.keys[2].value[2], 9.0f, 1e-6);

    const auto &r = back[0].channels[1];
    CHECK(r.path == AnimPath::Rotation);
    CHECK(r.interp == AnimInterp::Step);
    CHECK_NEAR(r.keys[1].value[3], 0.707f, 1e-6);

    CHECK_EQ(back[1].name, std::string("Idle"));
    CHECK(back[1].channels[0].path == AnimPath::Scale);

    // Determinism: same clips -> identical bytes.
    const auto path2 = (dir / "b.hanim").string();
    CHECK_EQ(WriteHAnim(path2, clips), 0);
    std::ifstream f1(path, std::ios::binary), f2(path2, std::ios::binary);
    std::string   s1((std::istreambuf_iterator<char>(f1)), {}),
        s2((std::istreambuf_iterator<char>(f2)), {});
    CHECK(s1 == s2);

    fs::remove_all(dir);
    return test::summary();
}
