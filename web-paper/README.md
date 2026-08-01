# TreeWork Web Paper

This directory contains the local reader for the TreeWork paper. The paper's
LaTeX source in `../paper/` remains authoritative; the generated article HTML
is rebuilt from it before development and production builds.

## Run locally

Requirements: Node.js 20 or newer, npm, and Pandoc.

```bash
cd web-paper
npm install
npm run dev
```

Open <http://127.0.0.1:8794/>.

## Build

```bash
npm run build
```

The static site is written to `../dist/web-paper/`. The Vite base path switches
to `/TreeWork/` in GitHub Actions so the same build can later be published with
GitHub Pages after review.

## Content flow

```text
paper/main.tex + paper/formal_guarantees.tex + paper/references.bib
                              |
                              v
                    npm run sync-paper
                              |
                              v
                 src/generated/paper.html
                              |
                              v
                       React reader UI
```

Do not hand-edit `src/generated/paper.html`; update the LaTeX source and run the
sync command instead.
