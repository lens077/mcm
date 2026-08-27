# Research: Programmatic Generation of Visio .vsdx (Rust exporter, task-dependency diagram)

Goal: emit `.vsdx` from a Rust core that Visio 2016+ opens with **no repair dialog**, with editable rectangle task shapes and connectors that stay **glued** when shapes move.

Primary evidence: [MS-VSDX] spec v2025-05-20 ([PDF](https://officeprotocoldoc.z19.web.core.windows.net/files/MS-VSDX/%5BMS-VSDX%5D-250520.pdf)); [Introduction to the Visio file format (.vsdx)](https://learn.microsoft.com/en-us/office/client-developer/visio/introduction-to-the-visio-file-formatvsdx); a genuine Visio-authored file dissected from the python [`vsdx`](https://github.com/dave-howard/vsdx) package template (`vsdx/media/media.vsdx`, contains masterless rectangles + glued Dynamic connectors — an ideal reference artifact).

## 1. Minimal valid OPC package structure

A `.vsdx` MUST be a ZIP conforming to OPC (ECMA-376/ISO 29500-2) — [MS-VSDX §2.1](https://officeprotocoldoc.z19.web.core.windows.net/files/MS-VSDX/%5BMS-VSDX%5D-250520.pdf); [LOC format entry](https://www.loc.gov/preservation/digital/formats/fdd/fdd000021.shtml). Parts observed in a real Visio file, with spec requirements ([MS-VSDX §2.3.1–2.3.3]):

| Part | Required? | Notes |
|---|---|---|
| `[Content_Types].xml` | **Required** (OPC) | See exact content below |
| `_rels/.rels` | **Required** | Package rel → `visio/document.xml`, type `http://schemas.microsoft.com/visio/2010/relationships/document` |
| `visio/document.xml` | **Required** — "exactly one Document XML part" (§2.3.3.3) | Root `VisioDocument` |
| `visio/_rels/document.xml.rels` | **Required** | Rels to pages (`…/relationships/pages`), optionally masters (`…/relationships/masters`), windows (`…/relationships/windows`) |
| `visio/pages/pages.xml` | **Required in practice** ("at most one Pages part", must be explicit rel target from Document, §2.3.3.9) | Root `Pages` |
| `visio/pages/_rels/pages.xml.rels` | **Required** | `…/relationships/page` → `page1.xml` |
| `visio/pages/page1.xml` | **Required** (target of explicit rel from Pages, §2.3.3.8) | Root `PageContents` |
| `visio/windows.xml` | Optional (not even in MS-VSDX's part enumeration — it is desktop UI state; Visio always writes it) | Root `Windows`; safe to include a stub |
| `docProps/app.xml`, `core.xml`, `custom.xml` | **Optional** — spec marks App/Core/Custom "optional part" (§2.3.2.1, §2.3.2.3, §2.3.2.4) | Visio writes them; include for cleanliness. `custom.xml` carries `IsMetric`, `BuildNumberEdited` |
| `visio/masters/masters.xml` + `masterN.xml` + rels | Optional — only if shapes reference a `Master` | `masters.xml` rel from document.xml; `masterN.xml` rel from masters.xml |

Exact `[Content_Types].xml` from a Visio-saved file (trim docProps/emf lines if you omit those parts):

```xml
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/visio/document.xml" ContentType="application/vnd.ms-visio.drawing.main+xml"/>
  <Override PartName="/visio/masters/masters.xml" ContentType="application/vnd.ms-visio.masters+xml"/>
  <Override PartName="/visio/masters/master1.xml" ContentType="application/vnd.ms-visio.master+xml"/>
  <Override PartName="/visio/pages/pages.xml" ContentType="application/vnd.ms-visio.pages+xml"/>
  <Override PartName="/visio/pages/page1.xml" ContentType="application/vnd.ms-visio.page+xml"/>
  <Override PartName="/visio/windows.xml" ContentType="application/vnd.ms-visio.windows+xml"/>
  <Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
  <Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>
  <Override PartName="/docProps/custom.xml" ContentType="application/vnd.openxmlformats-officedocument.custom-properties+xml"/>
</Types>
```

## 2. Namespaces and root elements

- All Visio parts in desktop files use `xmlns="http://schemas.microsoft.com/office/visio/2012/main"` plus `xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"` and `xml:space="preserve"` on the root ([intro doc](https://learn.microsoft.com/en-us/office/client-developer/visio/introduction-to-the-visio-file-formatvsdx); observed). (MS-VSDX documents the SharePoint "web drawing" namespace `…/visio/2011/1/core`; real files use `2012/main` — don't copy the spec's namespace.)
- Roots: `VisioDocument` (document.xml), `Pages` (pages.xml), `PageContents` (pageN.xml), `Masters`/`MasterContents`, `Windows`.
- Everything ShapeSheet-level is uniform `<Cell N="…" V="…" [U="…"] [F="formula"]/>`, `<Section N/IX>`, `<Row T/IX/N>` ([intro doc](https://learn.microsoft.com/en-us/office/client-developer/visio/introduction-to-the-visio-file-formatvsdx)).
- `document.xml` can be near-empty: `<VisioDocument …><DocumentSettings/></VisioDocument>` works; the Visio-saved reference adds `DocumentSettings` (GlueSettings=9, SnapSettings), `Colors`, `FaceNames`, and `StyleSheets` (IDs 0–6). If shapes reference `LineStyle='3'` etc., those StyleSheet IDs must exist; simplest exporter: omit style attributes on shapes and ship a `StyleSheet ID='0' NameU='No Style'` chain, or copy the small stylesheet block from a Visio-saved file.
- `pages.xml`: `<Page ID='0' NameU='Page-1' Name='Page-1'><PageSheet><Cell N='PageWidth' …/><Cell N='PageHeight' …/></PageSheet><Rel r:id='rId1'/></Page>` — note the `<Rel>` element binds the page to its part via `pages.xml.rels` (observed; §2.2.2.1).

## 3. page1.xml — shapes

Masterless rectangle exactly as Visio itself writes it (verified: the reference file's rectangles have **no `Master` attribute** and remain fully editable — masterless shapes are first-class):

```xml
<Shape ID='1' Type='Shape'>
  <Cell N='PinX' V='2'/><Cell N='PinY' V='9'/>
  <Cell N='Width' V='2'/><Cell N='Height' V='1'/>
  <Cell N='LocPinX' V='1' F='Width*0.5'/><Cell N='LocPinY' V='0.5' F='Height*0.5'/>
  <Cell N='Angle' V='0'/><Cell N='FlipX' V='0'/><Cell N='FlipY' V='0'/><Cell N='ResizeMode' V='0'/>
  <Section N='Geometry' IX='0'>
    <Cell N='NoFill' V='0'/><Cell N='NoLine' V='0'/><Cell N='NoShow' V='0'/><Cell N='NoSnap' V='0'/><Cell N='NoQuickDrag' V='0'/>
    <Row T='RelMoveTo' IX='1'><Cell N='X' V='0'/><Cell N='Y' V='0'/></Row>
    <Row T='RelLineTo' IX='2'><Cell N='X' V='1'/><Cell N='Y' V='0'/></Row>
    <Row T='RelLineTo' IX='3'><Cell N='X' V='1'/><Cell N='Y' V='1'/></Row>
    <Row T='RelLineTo' IX='4'><Cell N='X' V='0'/><Cell N='Y' V='1'/></Row>
    <Row T='RelLineTo' IX='5'><Cell N='X' V='0'/><Cell N='Y' V='0'/></Row>
  </Section>
  <Text>Task name
</Text>
</Shape>
```

`Rel*` rows are in shape-relative 0..1 coordinates; `MoveTo/LineTo` use drawing units; row `IX` starts at 1 and must be sequential ([Row element docs](https://learn.microsoft.com/en-us/office/client-developer/visio/row-element-geometry-sectionvisio-xml); MS-VSDX §Geometry example uses the same RelMoveTo/RelLineTo rectangle). MS-VSDX's own annotated example is exactly this pattern.

Tradeoff — masterless vs master: masterless = self-contained, no masters parts, still editable/moveable/glue-targetable; cost is repeated geometry per shape and no stencil semantics. A master is only *behaviorally* important for the connector (routing defaults, `MatchByName` dedup when users draw more connectors). Recommendation: **masterless rectangles + one "Dynamic connector" master**.

## 4. Connectors and glue

Observed glued connector (Visio-authored) — a 1-D shape instance of the Dynamic connector master:

```xml
<Shape ID='3' NameU='Dynamic connector' Type='Shape' Master='2'>
  <Cell N='PinX' V='3.5' F='Inh'/><Cell N='PinY' V='9.4' F='Inh'/>
  <Cell N='Width' V='2.17' F='GUARD(EndX-BeginX)'/><Cell N='Height' V='-1.19' F='GUARD(EndY-BeginY)'/>
  <Cell N='BeginX' V='2.41' F='_WALKGLUE(BegTrigger,EndTrigger,WalkPreference)'/>
  <Cell N='BeginY' V='10.05' F='_WALKGLUE(BegTrigger,EndTrigger,WalkPreference)'/>
  <Cell N='EndX'   V='4.58' F='_WALKGLUE(EndTrigger,BegTrigger,WalkPreference)'/>
  <Cell N='EndY'   V='8.86' F='_WALKGLUE(EndTrigger,BegTrigger,WalkPreference)'/>
  <Cell N='BegTrigger' V='2' F='_XFTRIGGER(Sheet.1!EventXFMod)'/>
  <Cell N='EndTrigger' V='2' F='_XFTRIGGER(Sheet.2!EventXFMod)'/>
  <Cell N='ObjType' V='2'/> <!-- from master: routable connector; GlueType 2 = walking glue -->
  <Cell N='ShapeRouteStyle' V='16'/><Cell N='ConFixedCode' V='6'/>
  <Section N='Geometry' IX='0'>
    <Row T='MoveTo' IX='1'><Cell N='X' V='0'/><Cell N='Y' V='0'/></Row>
    <Row T='LineTo' IX='2'><Cell N='X' V='2.17' F='Width*1'/><Cell N='Y' V='-1.19' F='Height*1'/></Row>
  </Section>
</Shape>
…
<Connects>
  <Connect FromSheet='3' FromCell='BeginX' FromPart='9'  ToSheet='1' ToCell='PinX' ToPart='3'/>
  <Connect FromSheet='3' FromCell='EndX'   FromPart='12' ToSheet='2' ToCell='PinX' ToPart='3'/>
</Connects>
```

- **Dynamic (walking) glue** = connector endpoint glued to the whole 2-D shape: `ToCell='PinX'`, `ToPart='3'` (whole shape); `FromPart` 9 = begin point, 12 = end point ([Connect element, Visio 2003 SDK value table](https://learn.microsoft.com/en-us/previous-versions/office/developer/office-2003/aa197578(v=office.11)); [Connect.ToCell](https://learn.microsoft.com/en-us/office/vba/api/visio.connect.tocell): "The pin of a 2D shape (creates dynamic glue)"; VSDX attrs: [Connect_Type](https://learn.microsoft.com/en-us/office/client-developer/visio/connect-element-connects_type-complextypevisio-xml)). Static glue to a connection point instead uses `ToCell='Connections.X1'`-style refs with `ToPart=100+row` and `PAR(PNT(Sheet.N!Connections.X2,…))` formulas in BeginX/EndX ([reverse-engineering example](https://ezhart.com/posts/visioreverseengineering); [bVisual connections walkthrough](https://bvisual.net/2016/08/09/understanding-visio-connections)).
- **Are Connect elements enough?** Yes for third-party files: for *untrusted* XML files "Visio uses the Connect elements to set glue formulas for shapes, similar to the GlueTo method" on open (geometry isn't rerouted until touched) — [Visio 2003 SDK Connect docs](https://learn.microsoft.com/en-us/previous-versions/office/developer/office-2003/aa197578(v=office.11)) (same DatadiagramML semantics carried into vsdx pages). So plain numeric `V` values + `<Connects>` re-establish glue. **Best practice: emit both** the `_WALKGLUE`/`_XFTRIGGER`/`GUARD` formulas *and* the `<Connect>` rows — that is byte-for-byte what Visio writes and what the python `vsdx` library replicates (it copies the templated connector, rewrites `Sheet.1!`/`Sheet.2!` in Beg/EndTrigger to the real shape IDs, and appends the two Connect elements — `connectors.py` in [dave-howard/vsdx](https://github.com/dave-howard/vsdx)).
- **Is a Dynamic connector master required?** Not strictly — a masterless 1-D shape (has `BeginX/BeginY/EndX/EndY`, `ObjType=2`, MoveTo/LineTo geometry) can carry the same glue formulas/Connects. But the master (Visio's built-in, `BaseID='{F7290A45-E3AD-11D2-AE4F-006008C9F5A9}'`, `MatchByName='1'`, master shape has `ObjType='2' GlueType='2' DynFeedback='2'`) gives correct routing behavior and lets user-drawn connectors merge with yours ([bVisual on auto-created Dynamic connector master](https://bvisual.net/2026/01/31/creating-a-dynamic-connector-master-automatically)). Minimal viable for editable glued straight lines: **copy the ~2.7 KB `master1.xml` Dynamic connector from a Visio-saved file** (as python `vsdx` does) and instance it per dependency. When using a master, add the page→master rel only if you mimic the template; the required wiring is document.xml.rels → masters.xml → master1.xml + both Content-Types overrides.

## 5. Units and coordinates

- Unqualified numeric cell values are drawing units = **inches**; page origin is **bottom-left**, y grows upward ([MS-VSDX §2.2.2.2 Coordinate System]). Metric files carry `U='MM'` display-unit hints only (values stay inches — A4 is stored as `PageWidth V='8.26771653543307'`).
- Page size lives in the page's `PageSheet`: `PageWidth`, `PageHeight` cells (US Letter 8.5×11, A4 8.2677×11.6929).
- `PinX/PinY` position the shape's rotation pin; `LocPinX/LocPinY` (usually `Width*0.5`, `Height*0.5`) place the pin at shape center. Text auto-wraps in the shape box; font size via a Character section (`<Section N='Character'><Row IX='0'><Cell N='Size' V='0.1666…'/>…` — size also in inches, 12 pt = 12/72 in) or inherit defaults by omitting it.

## 6. Repair-dialog pitfalls

- **Content types**: missing/incorrect `Override` for any `visio/*` part, or missing `Default` for `rels`/`xml` → "file is corrupt" repair. The `.vsdx` extension does not tolerate the macro content type (`…drawing.macroEnabled.main+xml` belongs to `.vsdm`) ([MS-VSDX §2.3.3.3]).
- **Relationships**: every part must be reached by the exact rel chain (package→document→pages→page); an orphaned page part or wrong rel `Type` string breaks open. "The package is a valid Visio file if it contains the correct set of parts and the relationships between the parts" ([intro doc](https://learn.microsoft.com/en-us/office/client-developer/visio/introduction-to-the-visio-file-formatvsdx)). Page `<Rel r:id>` must match the Id in `pages.xml.rels`.
- **Duplicate Shape IDs**: shape is *silently dropped* (no repair, but data loss); formulas reference shapes by ID (`Sheet.<ID>!`), so IDs must be unique per page and consistent with `Connects` ([Visio Guy on SO](https://stackoverflow.com/questions/63637877/visio-vsdx-format-unzip-and-zip-corrupts)).
- **Cell/section validity**: unknown `N` names, non-sequential Geometry `Row IX`, or a Geometry section without a leading (Rel)MoveTo row cause repair or invisible geometry ([Row docs](https://learn.microsoft.com/en-us/office/client-developer/visio/row-element-geometry-sectionvisio-xml)).
- **ZIP quirks**: naive unzip→rezip of a valid file with 7-Zip already yields "corrupt" ([SO report](https://stackoverflow.com/questions/63637877/visio-vsdx-format-unzip-and-zip-corrupts)) — the usual causes are zipping the containing folder (part names must be archive-root-relative), directory entries/`__MACOSX` junk, or exotic compression. Use plain store/deflate central-directory zips (Rust `zip` crate defaults are fine, as used by OOXML writers); write files, not folder entries; `[Content_Types].xml` conventionally first.
- **Encoding**: parts must be clean UTF-8; Visio writes no BOM — omit it. Keep `<?xml version="1.0" encoding="utf-8" ?>` declarations and `xml:space='preserve'` (text runs end with a literal `\n` inside `<Text>`).

## 7. Existing generators worth imitating

- **python [`vsdx`](https://github.com/dave-howard/vsdx)** (MIT): round-trips by keeping every part as parsed ElementTree and re-zipping; creates connectors by *cloning a Visio-authored template* (shapes + Dynamic connector master) rather than synthesizing XML — the strongest signal that template-cloning is the low-risk path. Its `vsdx/media/media.vsdx` is a ready-made minimal reference with masterless rectangles, glued straight + curved connectors, and all rels.
- **[vsdx-go](https://github.com/wijnberg-net/vsdx-go)** (Go): read/write/render incl. connectors and connection points — useful second reference for write-path details.
- **Rust**: [`libvisio-rs`](https://docs.rs/libvisio-rs) parses `.vsdx`/`.vsd` → SVG (**read-only**); no published Rust crate writes `.vsdx` (checked crates.io) — the exporter must be built on `zip` + an XML writer (e.g. `quick-xml`). [libvisio](https://github.com/LibreOffice/libvisio) (C++) is likewise import-only.
- **Aspose.Diagram** (commercial, .NET/Java/[Cloud](https://docs.aspose.cloud/diagram/overview)): full create-from-scratch API; its docs confirm the glue model (connect via PinX for dynamic glue) but it's not embeddable in a Rust desktop app.

## Minimum viable export checklist (N rectangles + M glued connectors)

1. Zip (deflate, no folder entries, no BOM) containing: `[Content_Types].xml`, `_rels/.rels`, `visio/document.xml` (+`_rels/document.xml.rels`), `visio/pages/pages.xml` (+`_rels/pages.xml.rels`), `visio/pages/page1.xml`, `visio/masters/masters.xml` + `master1.xml` (+ rels) copied from a Visio-saved Dynamic connector; optional `visio/windows.xml`, `docProps/*`.
2. `[Content_Types].xml`: `Default` for `rels`+`xml`; `Override` per Visio part with the exact `application/vnd.ms-visio.*+xml` types above; one Override per master/page part.
3. Rel chain: `.rels`→document (`…visio/2010/relationships/document`); document.rels→pages+masters(+windows); pages.rels→page1 with Id matching `<Rel r:id>`; masters.rels→master1.
4. `document.xml`: `VisioDocument` in `…/visio/2012/main` ns; include a StyleSheet block (or reference no styles); `pages.xml`: one `Page ID='0'` with `PageSheet` (`PageWidth`, `PageHeight` in inches) and `<Rel>`.
5. Each task: `<Shape ID=i Type='Shape'>` with PinX/PinY/Width/Height, LocPin at center, inline Geometry (RelMoveTo + 4 RelLineTo), `<Text>` — IDs unique, monotonically assigned; connectors take IDs from the same sequence.
6. Each dependency: connector `<Shape Master='2'>` with BeginX/BeginY at source border, EndX/EndY at target border (plain values), `Width F='GUARD(EndX-BeginX)'`, `Height F='GUARD(EndY-BeginY)'`, `_WALKGLUE` formulas on Begin*/End*, `BegTrigger/EndTrigger` `_XFTRIGGER(Sheet.<fromID>!EventXFMod)` / `(Sheet.<toID>!EventXFMod)`, MoveTo(0,0)+LineTo(Width,Height) geometry.
7. One `<Connects>` block: per connector two rows — `FromCell='BeginX' FromPart='9' ToSheet=<from> ToCell='PinX' ToPart='3'` and `FromCell='EndX' FromPart='12' ToSheet=<to> ToCell='PinX' ToPart='3'`.
8. Validate: unzip diff vs a Visio-saved file; open in Visio 2016+ (no repair prompt), drag a task → connectors must follow; re-save in Visio and diff again to learn what it normalizes.
