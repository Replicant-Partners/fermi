# DO NOT IMPORT THIS DIRECTORY INTO CRAFT

This folder is a container. It is **not** an AIUI project.

Craft treats the folder you select as the project root and looks for `app.json`
there. There is no `app.json` here, so importing this directory produces:

```
当前工程缺少 app.json，无法打包为 AIX
"The current project is missing app.json, cannot package as AIX"
```

...and Run Agent stays greyed out, because no valid project loaded.

## Import one of these instead

| Import this folder | What it is |
| --- | --- |
| `glasses/minimal_probe` | Smallest project that can render. Start here. |
| `glasses/hud_field_scout` | The real shell. Renders a provenance-marked card. |

After importing, the file tree should show **`app.json` at the top level**, with
no folder above it:

```
AGENTS.md
VERSION
app.js
app.json
package.json
pages/
  index/
    index.ink
```

If you can see two project names in the tree, you selected this directory and are
one level too high.

## Why this file exists

Because the mistake was mine. Two projects side by side under a shared parent,
with a guide at that level, makes selecting the parent the obvious move — and the
resulting error names the missing file rather than the wrong root, so it reads as
a problem with the project instead of with the selection.

See `RUNNING_IN_CRAFT.md` for the rest of the setup.
