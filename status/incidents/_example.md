---
# EXAMPLE ONLY. `example: true` keeps this file out of the build, so it never appears on the status page.
# Copy it to a new file named for the date and what broke, e.g. 2026-10-02-publish-errors.md, and delete this line
# and `example: true`.
example: true
title: Publishing failed for some organisations
start: 2026-09-20T10:05:00Z
end: 2026-09-20T11:20:00Z
environment: production
impact: degraded
components: [publishing, downloads]
updates:
  - at: 2026-09-20T10:12:00Z
    status: investigating
    text: |
      `cargo publish` is failing with HTTP 502 for some organisations. Registry reads and downloads of
      existing versions are unaffected.
  - at: 2026-09-20T10:48:00Z
    status: identified
    text: |
      A release upload retried after GitHub had already accepted it. We are deploying a fix.
  - at: 2026-09-20T11:20:00Z
    status: resolved
    text: |
      Fixed in 0.2.1. Publishes that failed can be retried; nothing needs cleaning up.
---
### What happened

Between 10:05 and 11:20 UTC, about 1 in 20 publishes failed. No crate was published twice or lost; the
index never pointed at a missing file.

### What we are changing

- Uploads are now idempotent: a retry finds the asset GitHub already has.
- A test covers a retry after a timed-out upload.
