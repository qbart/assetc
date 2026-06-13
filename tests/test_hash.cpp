#include "check.hpp"

#include "assetc/encode_mesh.hpp"
#include "assetc/hash.hpp"

using assetc::HashAssetRef;
using assetc::HashEmbedRef;

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

    // HashEmbedRef shares the FNV core + lowercasing, but the extension is part of
    // the ref, so it's significant where HashAssetRef (extension stripped) is not.
    CHECK_EQ(HashEmbedRef("scene/Level.JSON"), HashEmbedRef("scene/level.json")); // lowercased
    CHECK(HashEmbedRef("scene/level.json") != HashEmbedRef("scene/level.xml"));   // ext matters
    CHECK(HashEmbedRef("scene/level.json") != HashEmbedRef("config/level.json")); // path matters
    // An embed ref hashes identically to the same literal string via the shared core.
    CHECK_EQ(HashEmbedRef("scene/level.json"), HashAssetRef("scene/level.json"));

    return test::summary();
}
