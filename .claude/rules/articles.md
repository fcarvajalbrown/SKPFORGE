# Articles

Every article about skpforge gets its own folder, `docs/articles/<slug>/`, holding exactly:

- `article.md` — the article body, in the article's language. dev.to articles are English.
- `linkedin.md` — the LinkedIn post that shares it, **always in Spanish**, even when the article is English. It ends by pointing readers to the link with "El link está en la descripción." and carries a Markdown link to the most relevant image for the post, normally the folder's own `cover.png`.
- `cover.png` — the cover, made for this article with `tools/devto/make_cover.py`: a viewport-style before and after of a real input mesh and the real result, flat grey shading, thin wireframe, two plain labels with measured counts, no title. No neon, glow, HUD brackets, scanlines, letter-spaced kickers or accent bars: that poster look reads as machine-made. Source `cover.svg` sits beside it. Make both with `python tools/devto/make_cover.py --high <in.obj> --low <out.obj> --high-label "..." --low-label "..." --out docs/articles/<slug>`. Never use the MaskOps cover renderer here.

Before anything is shown to Felipe as final, both `article.md` and `linkedin.md` go through the `humanizer` skill and then the `voz-de-felipe` skill, in that order, on top of the global `felipe-outward-writing` rules. Nothing in them is invented: every fact and number comes from the repository or from Felipe.

Publishing to dev.to uses `tools/devto/devto_post.py` per the `devto-publish` skill: dry run first, live publish only on Felipe's go-ahead in that turn. dev.to needs the cover as a public URL, so a cover only goes up once the repository or another host serves `cover.png`.
