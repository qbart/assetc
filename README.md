# Asset Compiler

`assetc` compiles source assets (textures, meshes, shaders, materials, fonts) into runtime-friendly binary formats for video games. Supports octahedral-packed normals/tangents and meshletized geometry; mesh files reference materials by stable 64-bit hashes so the engine can share resources across assets. Mesh additionally emits a companion material table plus one content-addressed .ktx2 per referenced image. Embedded textures are written to a shared flat store keyed by content, so the same texture used by two different models is encoded once and shares one runtime handle.

Texture refs are stored as 64-bit hashes, not paths. A single global manifest maps each hash back to the file on disk. Load it once into a hash → path map and you can resolve any asset by a stable id - textures (baseColorTex etc.), but also meshes, materials, animations, fonts, shaders, and embeds - through one uniform lookup.
