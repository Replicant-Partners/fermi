# Making a Rokid glasses app for an ABW agent

**Status:** generator built and enforced. One shell registered (`hud_field_scout`).
**Code:** `src/glasses_shell.rs`, `examples/new_glasses_app.rs`, `tests/glasses_shell_parity.rs`
**Prior:** `HUD_AGENT_LAYERS.md` (the four layers, the hardware, why layer 2 is not scaffolded)

---

## 1. The measurement that produced this

`glasses/hud_field_scout/` was written by hand. Measured afterwards, its 319-line
page divides like this:

| | Lines | Contains |
|---|---|---|
| Invariant across any agent | ~290 | every trust rule |
| Varies per agent | ~29 | agent id, titles, the stub fixture, the query hint |

The invariant part is not boilerplate. It is:

- refusing to render a line that arrived without a provenance marker
- copying the server's marker rather than deriving one from `provenance`
- displaying the server's confidence band rather than forming an opinion
- a stub whose title says it is a stub
- a single-hue stylesheet, because the hardware has one channel
- an idle state that says something, because a blank card reads as a crash

**Every one of those is one careless line from being absent, and absent fails
silently.** A shell that derives its own markers still renders *a* marker. A
shell that drops the unstamped check still renders text. Only a reader comparing
a rendered card against its JSON would notice. This is the identical argument
`hud_contract` makes about a post-hoc grounding check, one layer further out: a
check that can be skipped is a check that will be.

An AIUI bundle is self-contained — no shared runtime to import — so app #2
*necessarily* contains its own copy of all of that. Hand-writing it means
retyping the doctrine, and retyping is where the fail-closed check does not make
it.

## 2. The pattern: generated, then drift-checked against the shipped bytes

```
src/glasses_shell.rs
  ShellSpec  ── 18 fields, the things that genuinely vary
  templates  ── the invariant 90%, stated once
       │
       │  render(spec) -> [app.json, app.js, package.json,
       │                   AGENTS.md, VERSION, pages/index/index.ink]
       ▼
glasses/<agent_id>/          committed generator output
       │
       ▼
tests/glasses_shell_parity.rs
  the_committed_shell_is_what_the_generator_produces   ← byte-for-byte
  every_app_directory_is_registered_or_exempt
  + 15 doctrine assertions against one instance
```

Two decisions carry the weight.

### 2.1 The parity test points at the shipped bytes, not at the template

A template validated against its own output is an idealisation of what shipped. A
template validated against the committed files is a claim about reality that can
be false — and was. The first run of the comparison found three real
divergences, none of which any other test would have caught:

| Found | What it was |
|---|---|
| `app.js` referenced `pages/card/index.ink` | a path that does not exist; stale from an earlier layout |
| manifest permissions had four-space nesting | drifted from the sample convention, and the upload validator's rules are not published |
| the manifest description was gone | collapsing the package and manifest descriptions onto one spec field silently dropped *"Edibility is never answered, because no source can supply it"* — the most load-bearing sentence on the only surface a wearer might read |

The third is the instructive one. The generator was *wrong* and the file on disk
was right. That is why `--check` and the failure message both say, in as many
words: decide which side is wrong before regenerating, because if the file on
disk is right then the template lost something and regenerating deletes it.

### 2.2 Byte-parity is what makes one instance sufficient

The 15 detailed doctrine assertions each examine a single app. That is sufficient
rather than lazy: once byte-parity holds, every registered shell is the same
template with different substitutions, so a property proved against one instance
is proved against all of them. **Without the parity test they would each be a
claim about one hand-written file** — which is exactly what they were before.

This is why registration, not directory presence, is what puts an app under
contract. `every_app_directory_is_registered_or_exempt` fails on an app with no
`ShellSpec`, because an unchecked app is the hand-written copy the generator
exists to prevent.

## 3. Workflow

```sh
# see what would change, write nothing
cargo run --example new_glasses_app -- --check

# regenerate every registered shell
cargo run --example new_glasses_app

# one shell
cargo run --example new_glasses_app -- hud_field_scout
```

Adding an app:

1. Give the agent **field contracts** in `src/grounding_trust.rs`. Not optional —
   `the_registered_agent_has_field_contracts` refuses a shell for an agent with
   none. A card for an uncontracted agent renders every line unmarked, and
   unmarked is the treatment reserved for verified retrieval. The friendliest
   possible failure is the correct one.
2. Add a `ShellSpec` to `SHELL_SPECS`.
3. Run the generator.
4. `cargo test --test glasses_shell_parity`.

Regeneration is deliberately destructive of hand edits. The generated app is the
display surface for a trust boundary; a local edit that removed the fail-closed
check would be invisible, because the shell would still render markers. Losing
the edit is a better outcome than keeping it.

## 4. Details worth not rediscovering

**`provenance: 'x'` in the stub is invalid on purpose.** A stub carrying a
plausible provenance verdict would demonstrate a provenance pipeline that had not
run. `x` is in no vocabulary, and
`the_stub_lines_carry_an_invalid_provenance` fails if it becomes one.

**A surviving `__PLACEHOLDER__` panics rather than shipping.** The failure it
prevents is specific: a renamed placeholder leaves `__AGENT_ID__` in a `fetch()`
URL, the app builds, Craft renders it, and the request 404s against an agent
named `__AGENT_ID__`. That reads as a backend problem for as long as anyone is
willing to look at the backend.

**Placeholders, not `format!`.** The `.ink` templates are full of `{{ }}` runtime
interpolation. Escaping every brace across 300 lines is a transcription error
waiting to happen — one that produces a page which compiles and renders
`{{ item.text }}` as literal text.

**`r##"` not `r#"`.** `"backgroundColor": "#000000"` contains `"#`, which
terminates a `r#"` raw string. This cost a compile error at a line number 200
lines from the cause.

**Two descriptions, not one.** `package_description` describes the bundle;
`manifest_description` describes what the agent does and is shown to a person.
See §2.1.

**Failure output has to be readable.** The parity check first used `assert_eq!`
on two documents and printed sixteen kilobytes of escaped `.ink`, burying the one
changed line. A check whose output cannot be read is a check someone reruns with
the expected value pasted in, which is the same as not having it. It now names
the line and both sides of it.

## 5. What is still hand-work, and should stay that way

The generator produces the **surface**. It does not produce the agent, and should
not:

- the agent card and system prompt
- field contracts in `grounding_trust.rs` — the substantive claim about what each
  field's evidence is
- the `hud_contract` card shape the agent returns

That split is the point. The display layer is mechanical and its rules are
universal; the grounding claims are specific and must be argued per field.

## 6. Open: the contract is still foraging-shaped

`hud_contract::SAFETY_BLOCK` is the literal string `"edibility"`, and
`SAFETY_LEAKS` is a list of needles about poison. Generic to a *card*, specific
to *foraging*.

For a second agent this is wrong in both directions. A weather card would be
scanned for the word "toxic" and would never trip it — a check that can only
return clean, i.e. §5.1's "a check that has never failed has not been tested". And
a second safety-bearing domain would need its own needles, which under the
current shape means editing a constant shared with an agent that does not want
them.

The fix is a per-agent `CardProfile` declaring which block is safety-bearing and
which needles apply, resolved the way `is_declared_gap` already resolves by
`agent_id`. Not done, because it should be done *with* the second agent in hand
rather than guessed at — the same reason `LINE_MAX` was left at 60 rather than
changed blind.

**Recommended second agent: `weather_oracle`.** It has 12 field contracts
already, real tools so its provenance is genuinely mixed rather than uniformly
inferred, no camera, and no safety surface. It also makes the marker column
*informative* instead of safety-critical: a forecast whose drivers are all `~` is
a very different object from one whose drivers are sourced, and that is the
general case this whole mechanism is for.
