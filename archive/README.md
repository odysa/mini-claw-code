# Archive

V1 material kept for reference. The current course is [V2 — the 18-chapter
"Claude Code Architecture in Rust"](../mini-claw-code-book/src/ch00-overview.md)
under `mini-claw-code-book/src/`.

```
archive/
├── v1-book/        Old V1 chapter prose (hands-on 15 + 6 extensions) and its Chinese translation.
└── v1-tests/       Notes on the V1 test numbering that still appears in the live test files.
```

## What's here and why

- **`v1-book/en/`** — the original V1 English tutorial (22 chapters, Part I hands-on
  + Part II extensions + Part III production-grade). Superseded by V2.
- **`v1-book/zh/`** — Chinese translation of V1 (chapters 1–13 translated in full;
  14–21 were 117-byte placeholders). Includes the `book.zh.toml` mdbook config
  and `README.zh.md`. Kept so a future V2 retranslation can start from real
  prior work.
- **`v1-book/assets/`** — old language switcher (`lang-switcher.js/css`) and
  landing page (`lang-landing.html`), used only when both EN and ZH books were
  live.
- **`v1-tests/README.md`** — explanation of the `v1_` prefix on live starter
  test files. The tests themselves still run; only their naming is V1-legacy.

These files are not linked from any active `SUMMARY.md` and do not render in
the mdbook output. They are plain markdown — open in an editor or preview to
read directly.
