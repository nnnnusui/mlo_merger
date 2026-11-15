# Vanilla Clothing Collection

1. Replace the public extraction-extension option with `--generate-vanilla-clothing`,
   owned by `cli/clothing` and `core/clothing/vanilla`.
2. Reuse vanilla's archive traversal and versioned publication with an internal file
   selection, keeping normal vanilla extraction limited to YMAP/YBN.
3. Identify all Ped variation YMTs by their `CPedVariationInfo` root in META, PSO,
   RBF or XML, keeping raw files and vanilla's same-name overlay behavior.
4. Test selection, CLI defaults, overlays and existing vanilla workflows; update
   current-use documentation and run formatting and Clippy.
