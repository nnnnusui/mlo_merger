# Parent Reference Sample

These small XML fixtures exercise parent-index repair without depending on a particular game resource.

- `vanilla_parent.ymap.xml` contains parent entities 100 and 200.
- `resource_a_parent.ymap.xml` is unchanged from vanilla; its child uses parent index 1 for GUID 200.
- `resource_b_parent.ymap.xml` removes GUID 100. The merged parent therefore places GUID 200 at index 0.
- `child.ymap.xml` is used as both a mod clone and a vanilla dependent child. Both must be rewritten from index 1 to index 0.

The merge regression creates a placeholder source binary in a temporary directory. No original game assets are required. The separate ignored Native/CodeWalker test uses these XML outputs and loads schemas generically from the local extracted YMAP directory.
