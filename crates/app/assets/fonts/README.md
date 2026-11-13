Fonts embedded in the termden binary and registered at startup, so a fresh
machine renders correctly without installing anything. Installed copies of
the same families still work; these only fill the gap.

- Noto Sans Regular/Medium/Bold (UI): SIL Open Font License 1.1
- JetBrainsMono Nerd Font Regular/Bold (terminal): SIL Open Font License 1.1
  (Nerd Fonts glyphs: MIT)
- Material Symbols Rounded (icons), subset to the glyphs termden uses:
  Apache License 2.0

Add a new icon's codepoint to the subset when adding it to `widgets::icons`,
then regenerate (keep ASCII and the name table: without them GPUI renders
the icons as empty boxes):

    pyftsubset 'MaterialSymbolsRounded[FILL,GRAD,opsz,wght].ttf' \
      --unicodes=U+0020-007E,U+E145,U+E15B,U+E5CA,U+E5CD,U+E5CF,U+E5D1,U+E5D5,U+E5CC,U+E5CB,U+E8B8,U+EB8E,U+E931,U+E3C1,U+E3E0,U+E5D0 \
      --layout-features='' --name-IDs='*' --notdef-outline \
      --output-file=MaterialSymbolsRounded-termden.ttf
