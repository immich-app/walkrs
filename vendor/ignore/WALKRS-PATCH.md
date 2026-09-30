This is the published `ignore` 0.4.33 crate, licensed under MIT or the Unlicense.
The upstream license files are preserved here.

The optional `directory-summary` feature adds default methods to
`ParallelVisitor`. It sees the existing raw directory listing and its completion
status before children are scheduled. A second callback can refine the byte for
each child on the same producer before dispatch. The application-defined byte
is stored on each immediate child's `DirEntry`, preserving context across work
stealing. Producer-local state is never consulted by stolen child callbacks.
Roots and the sequential iterator carry zero. No names, entry handles, or maps
are retained by this extension, and traversal/filtering rules are unchanged.

Only `Cargo.toml` and `src/walk.rs` are patched. Keep the dependency version
pinned and reapply/review this small extension when updating upstream. The
feature is a local prototype and has not been accepted upstream.
