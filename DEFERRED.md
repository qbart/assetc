# Deferred

Ideas checked and parked for later, with what is known so far.

## `.obj` models

Preview (and later compile) Wavefront OBJ files like glTF models (see
`docs/features/models.md`). Nothing in the dependency tree reads OBJ: it
needs a crate such as `tobj` or a small parser of our own (positions,
normals, UVs, faces; `.mtl` materials optional). The file tree already shows
`.obj` files with the `torus` icon.
