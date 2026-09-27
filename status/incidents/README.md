# Incidents

Each incident is one Markdown file in this directory, bundled into the status Worker when it is built and shown on
<https://status.privatecrates.dev> (production) or <https://status.privatecrates.dev/dev> (dev), and in the Atom feed
at `/feed.xml` (`/dev/feed.xml`). There is no admin page: posting or updating an incident is a commit and a deploy.

`_example.md` shows every field. It has `example: true`, which keeps it out of the build; copy it rather than
editing it.

## Writing one

Name the file for the day it started and what broke, lower case with hyphens: `2026-10-02-publish-errors.md`. The
name becomes its anchor (`/#incident-2026-10-02-publish-errors`) and its feed id, so do not rename it once posted.

```markdown
---
title: Publishing failed for some organisations
start: 2026-10-02T10:05:00Z
# end: 2026-10-02T11:20:00Z     (leave out while it is ongoing)
environment: production          # or dev; default production
impact: degraded                 # or outage; default degraded
components: [publishing]         # reads, downloads, publishing, signin, website, billing
updates:
  - at: 2026-10-02T10:12:00Z
    status: investigating        # investigating, identified, monitoring, resolved or update
    text: |
      What customers see, what still works, and when we will say more.
---
An optional write-up, in Markdown, usually added once it is resolved.
```

- **Times** are UTC with a zone (`Z`), to the minute at least.
- **While an incident has no `end`** (or its `end` is in the future), it is active: it is shown at the top of the
  page, and its components are shown as at least its `impact`, whatever the probes say. Remember to close it.
- **Updates** can be in any order in the file; the page shows the newest first. Say what customers see and what
  still works, not internal detail. Never include organisation names, crate names or tokens.
- **Markdown** is a small subset: paragraphs, `-` and `1.` lists, `###` headings, `code`, `**bold**`, `*italic*` and
  `[links](https://…)`. Anything else is shown as text.
- A mistake (an unknown component, a time without a zone, a misspelt field) fails the build with the file name and
  the problem, so it cannot reach the page.

## Posting and updating

From `status/`:

```sh
pnpm install --frozen-lockfile
pnpm build          # checks every incident file; fails on a mistake
pnpm test
pnpm exec wrangler deploy
```

`wrangler deploy` runs the build again and uploads the new version; the page shows it within seconds. Commit the
file too, so the next deploy keeps it. For each update, add an entry under `updates` (and `end` when it is over) and
deploy again. See `docs/deploy.md` §14.
