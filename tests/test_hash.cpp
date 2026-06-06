#include "check.hpp"

#include "assetc/encode_mesh.hpp"

using assetc::HashAssetRef;

int main()
{
    // Canonical FNV-1a 64 test vectors (offset basis for empty, known vector for "a").
    CHECK_EQ(HashAssetRef(""), 0xcbf29ce484222325ULL);
    CHECK_EQ(HashAssetRef("a"), 0xaf63dc4c8601ec8cULL);

    // HashAssetRef lowercases ASCII, so case must not change the hash.
    CHECK_EQ(HashAssetRef("A"), HashAssetRef("a"));
    CHECK_EQ(HashAssetRef("Models/Court/Tex_0"), HashAssetRef("models/court/tex_0"));

    // Deterministic and order/content sensitive.
    CHECK_EQ(HashAssetRef("court/tex_0"), HashAssetRef("court/tex_0"));
    CHECK(HashAssetRef("court/tex_0") != HashAssetRef("court/tex_1"));

    // Locks the project canonical texture-ref form to its on-disk value (regressions
    // in the canonical string would silently break .hman <-> .hmat parity).
    CHECK_EQ(HashAssetRef("court/tex_0"), 0xad44c4a2970495b9ULL);
    CHECK_EQ(HashAssetRef("court/tex_1"), 0xad44c3a297049406ULL);

    return test::summary();
}
