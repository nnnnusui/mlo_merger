# Parent Reference Sample

These small fixtures exercise parent-index repair without depending on a particular game resource. Each `.ymap` is the RSC7 binary built from the `.ymap.xml` next to it, which is kept as the readable source; `../hash_names.json` resolves their archetype names.

- `vanilla_parent.ymap` contains parent entities 100 and 200.
- `resource_a_parent.ymap` is unchanged from vanilla; its child uses parent index 1 for GUID 200.
- `resource_b_parent.ymap` removes GUID 100. The merged parent therefore places GUID 200 at index 0.
- `child.ymap` is used as both a mod source and a vanilla dependent child. Both must be rewritten from index 1 to index 0.

The merge regressions copy these binaries into a temporary directory, so no original game assets are required. The separate ignored Native/CodeWalker tests rebuild the merged binaries through CodeWalker.
