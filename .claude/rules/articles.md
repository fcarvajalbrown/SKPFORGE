# Articles

Every article about skpforge gets its own folder, `docs/articles/<slug>/`, holding exactly:

- `article.md` — the article body, in the article's language. dev.to articles are English.
- `linkedin.md` — the LinkedIn post that shares it, **always in Spanish**, even when the article is English. It ends by pointing readers to the link with "El link está en la descripción.", then 3 to 5 hashtags, then a Markdown link to the most relevant image for the post, normally the folder's own `cover.png`.
  - **Short.** About 120 words at most, around 700 to 800 characters. The post is a teaser for the article, never a summary of it. Felipe's call: published studies put LinkedIn's engagement sweet spot at 1,200 to 2,000 characters (AuthoredUp, 372,126 posts; Richard van der Blom, 1.8 million), but he wants these short, and that wins.
  - **Hook under 140 characters.** Only the first ~140 characters show on mobile before "see more" (~210 on desktop), so the first line carries the post. Never open with a question or with "Yo".
  - **Hashtags: 3 to 5**, broad English ones for reach plus one niche one, for example `#Rust #GameDev #UnrealEngine #SketchUp #Retopología`. Metricool's study of 673,658 posts found at least one relevant hashtag lifts impressions by 85%, with 1 to 5 the best range.
- `cover.png` — the cover, made for this article with `tools/devto/make_cover.py`: a viewport-style before and after of a real input mesh and the real result, flat grey shading, thin wireframe, two plain labels with measured counts, no title. No neon, glow, HUD brackets, scanlines, letter-spaced kickers or accent bars: that poster look reads as machine-made. Source `cover.svg` sits beside it. Make both with `python tools/devto/make_cover.py --high <in.obj> --low <out.obj> --high-label "..." --low-label "..." --out docs/articles/<slug>`. Never use the MaskOps cover renderer here.

Before anything is shown to Felipe as final, both `article.md` and `linkedin.md` go through the `humanizer` skill and then the `voz-de-felipe` skill, in that order, on top of the global `felipe-outward-writing` rules. Nothing in them is invented: every fact and number comes from the repository or from Felipe.

Publishing to dev.to uses `tools/devto/devto_post.py` per the `devto-publish` skill: dry run first, live publish only on Felipe's go-ahead in that turn. dev.to needs the cover as a public URL, so a cover only goes up once the repository or another host serves `cover.png`.
