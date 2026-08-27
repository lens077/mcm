# XMind (.xmind) File Format — Export Research for the Rust Core

Target: emit `.xmind` files that XMind 2020/Zen → XMind 2026 opens cleanly as **editable** mind maps.
Primary sources: official [xmindltd/xmind-generator](https://github.com/xmindltd/xmind-generator) (modern, recommended by XMind), official [xmindltd/xmind-sdk-js](https://github.com/xmindltd/xmind-sdk-js), [xmindparser](https://github.com/tobyqin/xmindparser).

## 1. ZIP package structure

A `.xmind` file is a plain ZIP archive ([simple-mind-map docs](https://wanglin2.github.io/mind-map-docs/en/api/xmind.html)). Entries seen in the wild ([filext](https://filext.com/file-extension/XMIND)) and in official generators ([serializer.ts](https://github.com/xmindltd/xmind-generator/blob/main/src/internal/serializer.ts)):

| Entry | Required? | Notes |
|---|---|---|
| `content.json` | **Yes** | The whole mind-map model (array of sheets). |
| `metadata.json` | **Yes** (both official generators always emit it) | Minimal: `{"creator":{"name":"xmind-generator"},"dataStructureVersion":"2"}`. Real XMind files add `activeSheetId`, creator version, etc. |
| `manifest.json` | **Yes** | `{"file-entries":{"content.json":{},"metadata.json":{}, "resources/<file>":{} ...}}` — must list every payload entry. |
| `Thumbnails/thumbnail.png` | No | XMind writes it; generators omit it and files still open. |
| `resources/<sha256>.<ext>` | Only if images/attachments used | Referenced from topics as `"xap:resources/<file>"`. |
| `content.xml` | No (legacy) | xmind-sdk-js embeds a canned XMind-8 "warning" map for graceful degradation in XMind ≤8 ([dumper.ts](https://github.com/xmindltd/xmind-sdk-js/blob/master/src/utils/dumper.ts)); the newer official xmind-generator omits it entirely. |

## 2. content.json schema essentials

From the official serializer ([serializer.ts](https://github.com/xmindltd/xmind-generator/blob/main/src/internal/serializer.ts)):

- **Top level**: JSON **array** of sheet objects.
- **Sheet**: `{ "id", "class": "sheet", "title", "rootTopic", "relationships"?: [] }`.
- **Topic**: `{ "id", "class": "topic", "title", "notes"?, "labels"?: [string], "children"?: { "attached": [Topic], "summary"?: [Topic] }, "markers"?: [{"markerId": string}], "summaries"?: [{"id","class":"summary","range":"(0,2)","topicId"}], "image"?: {"src":"xap:resources/<file>"}, "href"? }`.
- **Relationship** (non-hierarchical link): `{ "id", "class": "relationship", "end1Id": <topicId>, "end2Id": <topicId>, "title" }`.
- Summary `range` is a string `"(start,end)"` over the sibling index range of `children.attached`; the summary's own topic lives in `children.summary` and is pointed to by `topicId`.

Minimal valid `content.json` (mirrors exactly what xmind-generator emits, which XMind opens):

```json
[
  {
    "id": "9f2a6c1e-1b7d-4e0a-9c1d-2f3b4a5c6d7e",
    "class": "sheet",
    "title": "Sheet 1",
    "rootTopic": {
      "id": "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d",
      "class": "topic",
      "title": "Project Plan",
      "children": {
        "attached": [
          { "id": "1b2c3d4e-5f6a-4b7c-8d9e-0f1a2b3c4d5e", "class": "topic", "title": "Milestone 1",
            "markers": [{ "markerId": "task-done" }] },
          { "id": "2c3d4e5f-6a7b-4c8d-9e0f-1a2b3c4d5e6f", "class": "topic", "title": "Milestone 2",
            "notes": { "plain": { "content": "Ship by Q3\n" } },
            "labels": ["phase-2"] }
        ]
      }
    },
    "relationships": [
      { "id": "3d4e5f6a-7b8c-4d9e-a0f1-2b3c4d5e6f7a", "class": "relationship",
        "end1Id": "1b2c3d4e-5f6a-4b7c-8d9e-0f1a2b3c4d5e",
        "end2Id": "2c3d4e5f-6a7b-4c8d-9e0f-1a2b3c4d5e6f",
        "title": "blocks" }
    ]
  }
]
```

## 3. Notes, labels, markers

- **Notes**: plain form is `notes: { "plain": { "content": "text\n" } }` — xmind-generator emits only this (appends a trailing `\n`). Rich form adds `html: { "content": { "paragraphs": [ { "spans": [ { "text": "..." } ] } ] } }` and optionally a Quill-delta-like `ops: { "ops": [ { "insert": "..." } ] }`; xmind-sdk-js emits all three ([note.ts](https://github.com/xmindltd/xmind-sdk-js/blob/master/src/core/note.ts)). Plain-only is sufficient and editable.
- **Labels**: `labels: ["string", ...]` — plain strings on the topic.
- **Markers**: `markers: [{"markerId": "<id>"}]`. Built-in ids from the official [marker.ts](https://github.com/xmindltd/xmind-generator/blob/main/src/marker.ts) and [sdk marker constants](https://github.com/xmindltd/xmind-sdk-js/blob/master/src/common/constants/marker.ts):
  - `priority-1` … `priority-9` (7–9 hidden in newer UI)
  - `task-start`, `task-oct` (⅛), `task-quarter`, `task-half`, `task-done`, `task-pause` (sdk constants also list `task-3oct`, `task-3quar`, `task-7oct`)
  - `flag-red`, `flag-orange`, `flag-dark-blue`, `flag-purple`, `flag-green`, `flag-blue`, `flag-gray`
  - `star-…` and `people-…` with the same seven color suffixes
  - `smiley-laugh|smile|cry|surprise|boring|angry|embarrass`
  - `arrow-left|right|up|down|left-right|up-down|refresh`
  - `month-jan` … `month-dec`; `week-sun|mon|tue|web|thu|fri|sat` (note: Wednesday really is `week-web` in the official SDK)
  - One marker per group per topic; adding a same-group marker replaces it (enforced by `isSameGroup` in the generator).

## 4. Missing optional entries

XMind opens files without `Thumbnails/`, `resources/`, or `content.xml`: the official xmind-generator emits only `content.json` + `metadata.json` + `manifest.json` (plus `resources/` when images exist) and its stated purpose is producing files the XMind apps open ([README](https://github.com/xmindltd/xmind-generator)). Missing thumbnail only means no preview icon. `resources/` files referenced by `image.src` but absent from the ZIP/manifest will break image display — omit the `image` field instead.

## 5. Programmatic-generation pitfalls

- **IDs**: opaque strings; must be unique within the file (relationships/summaries reference them). Official generator uses lowercase UUID v4 ([common.ts](https://github.com/xmindltd/xmind-generator/blob/main/src/internal/common.ts)); XMind itself uses 26-char base-36 strings — either works.
- **Required fields**: every sheet/topic needs `id` and `title` (generator emits `title: ""` rather than omitting it); include `"class"` (`sheet`/`topic`/`relationship`/`summary`) as the official generator does.
- **Encoding**: strict UTF-8, no BOM; ZIP entry names ASCII (keep the canonical lowercase names exactly — `content.json`, not `Content.json`).
- **Compression**: both official libraries generate the ZIP with `compression: 'STORE'` ([zipper.ts](https://github.com/xmindltd/xmind-sdk-js/blob/master/src/utils/zipper.ts), [serializer.ts](https://github.com/xmindltd/xmind-generator/blob/main/src/internal/serializer.ts)). Deflate is also accepted (XMind's own saves are compressed per [filext](https://filext.com/file-extension/XMIND)), but STORE is the proven-safe choice for a Rust `zip` crate exporter. No encryption, no zip64 needed for normal sizes.
- **Manifest completeness**: `manifest.json`'s `file-entries` must list `content.json`, `metadata.json`, and every `resources/*` entry (empty-object values).
- **"File corrupted" triggers**: malformed JSON, `content.json` not being a top-level array, duplicate/missing ids referenced by relationships, resources referenced but missing, or a ZIP whose entries can't be read. The multilingual warning map baked into sdk-js's `content.xml` exists precisely so *old* XMind 8 shows a warning instead of garbage — modern XMind ignores it.
- **File ordering**: no ordering requirement observed; jszip writes insertion order and both orders (content-first) are accepted.

## 6. Version differences 2023–2026

- `content.json` (`dataStructureVersion: "2"`) has been stable from XMind Zen/2020 through XMind 2026: xmindparser parses "Xmind Zen and Xmind 2026" with the same `content.json` reader ([xmindparser](https://github.com/tobyqin/xmindparser), [PyPI](https://pypi.org/project/xmindparser)).
- Newer releases *added* optional structures (callouts, boundaries, stickers-as-images, redesigned To-Do/Task in the 2026 release per [XMind user guide](https://xmind.com/user-guide/enrich-a-topic)) — all additive; a minimal exporter ignoring them stays compatible.
- Legacy `content.xml` is **only** needed if XMind 8 (≤2019) must open the file; XMind 2020+ never requires it ([smithery xmind skill](https://smithery.ai/skills/apeyroux/xmind), sdk-js dumper). Recommendation: skip it.

## 7. Reference open-source generators

- **[xmindltd/xmind-generator](https://github.com/xmindltd/xmind-generator)** (official, modern, TS): emits `content.json` + `metadata.json` + `manifest.json` (+ `resources/`), STORE zip, UUID ids. **Best template for our Rust exporter.**
- **[xmindltd/xmind-sdk-js](https://github.com/xmindltd/xmind-sdk-js)** (official, older `xmind` npm pkg): same three JSON files plus legacy `content.xml` fallback; richer notes (plain+html+ops), summaries, markers.
- **[tobyqin/xmindparser](https://github.com/tobyqin/xmindparser)** (Python, read-only): confirms Zen→2026 read path is just `content.json` out of the ZIP.
- xmindltd/xmind-sdk-python and [leungwensen/xmind-sdk-javascript](https://github.com/leungwensen/xmind-sdk-javascript): legacy XML format only — do not copy.

## Minimum viable export checklist (Rust exporter must emit)

- [ ] ZIP archive (extension `.xmind`), entries **stored** (no compression), UTF-8 JSON, exact lowercase entry names.
- [ ] `content.json`: top-level **array**; each sheet `{id, class:"sheet", title, rootTopic}`.
- [ ] Topics: `{id, class:"topic", title}`; hierarchy via `children.attached: [...]`.
- [ ] Unique string ids everywhere (UUID v4 is fine); never reuse or omit.
- [ ] Optional per topic: `notes.plain.content` (trailing `\n`), `labels: [..]`, `markers: [{markerId}]` using exact built-in ids (`task-done`, `flag-red`, `priority-1`, …).
- [ ] Cross-links: sheet-level `relationships: [{id, class:"relationship", end1Id, end2Id, title}]`.
- [ ] `metadata.json`: at minimum `{"creator":{"name":"<our-app>"},"dataStructureVersion":"2"}`.
- [ ] `manifest.json`: `{"file-entries":{"content.json":{},"metadata.json":{}}}` plus one key per `resources/*` file.
- [ ] Images/attachments (if any): store as `resources/<sha256>.<ext>`, reference via `image.src = "xap:resources/<file>"`, list in manifest.
- [ ] Skip `Thumbnails/` and `content.xml` — not required by XMind 2020–2026.
