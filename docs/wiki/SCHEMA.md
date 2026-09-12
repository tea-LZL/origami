---
title: Wiki schema
type: schema
status: current
updated: 2026-09-12
---

# Origami wiki — schema and conventions

This directory is an LLM-maintained knowledge wiki about the **Origami** project.
It sits between the raw sources (this repository) and anyone asking questions about
it. The LLM owns this layer; the human curates sources, directs the analysis, and
reads the result.

## Three layers

| Layer | Location | Owner | Rule |
|---|---|---|---|
| Raw sources | `crates/`, `ui/`, `tests/`, `docs/`, `.hermes/plans/`, root docs | project | **Immutable to the wiki.** Never edit a source to make the wiki easier to write. |
| Wiki | `docs/wiki/**` | LLM | Freely created, updated, cross-linked. |
| Schema | `docs/wiki/SCHEMA.md` + root `AGENTS.md` pointer | human + LLM | Co-evolve here, not in the pages. |

`MEMORY.md` no longer holds knowledge: the wiki supersedes it. It is now a pointer
into this wiki. Do not re-grow it.

## Layout

```
docs/wiki/
  SCHEMA.md        this file
  index.md         content catalog — read this first on every query
  log.md           append-only chronology of ingests, queries, lint passes
  overview.md      evolving synthesis / current state of the project
  architecture/    how the system works today, subsystem by subsystem
  decisions/       distilled, cross-linked decision records (ADRs and locked calls)
  concepts/        reusable mechanisms and domain ideas
  entities/        crates, the UI, and external dependencies
  status/          release readiness, build/verification, known drift
  sources/         one bounded summary page per ingested document
```

## Page conventions

Every page starts with YAML frontmatter:

```yaml
---
title: Sync engine
type: architecture      # architecture | decision | concept | entity | status | source | overview
status: current         # current | evolving | target | stale | superseded
updated: 2026-09-12     # ISO date of last real edit
sources:                # repo-relative raw sources this page is accountable to
  - docs/adr/0003-sync-algorithm.md
  - crates/origami-core/src/sync.rs
---
```

- **Filenames are kebab-case and globally unique.** Obsidian resolves `[[wikilinks]]`
  by basename, so uniqueness across the whole vault is what keeps links unambiguous.
- **Link with `[[basename]]`** and link liberally. A page with no inbound links is a
  defect (see Lint).
- **Cite evidence.** Any non-obvious claim carries the source path or the page it came
  from. Distinguish *what the code does now* from *what a plan intends*: mark the latter
  `status: target` or label it inline as **target state**.
- **Never paste a whole source.** Summarize and link to the source file.
- Dates are ISO `YYYY-MM-DD`.

## Precedence when sources disagree

1. Code and tests — what *is*.
2. ADRs and locked decisions — *why* it is that way.
3. `docs/PLAN.md`, `docs/IMPROVEMENT_PLAN.md` — target state and backlog.
   Checkbox state is **not** release evidence.
4. `.hermes/plans/*` — historical execution intent for work already largely done.

When code and a plan disagree: code wins for "current", the plan wins for "intended",
and the disagreement is recorded in [[known-drift]].

## Workflows

### Ingest

1. Read the source; discuss key takeaways with the human when they are present.
2. Write **one bounded summary page** under `sources/`, named after the source file.
3. Extract claims and update the *substantive* pages they touch —
   [[overview]], `architecture/`, `concepts/`, `entities/`, `decisions/`, `status/`.
   A source typically touches 5–15 pages; updating only its own summary page is a defect.
4. Record contradictions explicitly (a "Contradiction" callout on both pages, plus
   [[known-drift]] when the conflict is between code and plan).
5. Update [[index]]; append an entry to [[log]].

Ingest is done when: summary page exists, touched pages are updated, index and log
reflect it, and every new claim cites its source.

### Query

1. Read [[index]] first; it is the retrieval path at this scale.
2. Read the candidate pages. Drill into raw sources when a claim is load-bearing and
   the page is not `status: current`.
3. Answer with citations to both wiki pages and raw file paths.
4. **File reusable answers back.** A comparison, analysis, or synthesis the human asked
   for becomes a wiki page; then index and log it. Explorations must compound.

### Lint

Ask for a lint pass; check and report:

- contradictions between pages;
- claims newer sources have superseded (`status: stale` candidates);
- orphan pages with no inbound links;
- important concepts mentioned but lacking their own page;
- missing cross-references;
- unsourced or unverifiable claims;
- dead `sources:` paths or renamed files;
- index drift (pages absent from [[index]]);
- log gaps (changes with no entry).

Fix the cheap defects in the pass; list the rest as follow-ups. Log the lint pass.

## Working rules

- Read `docs/adr/` before changing identity, backend, or sync semantics.
- Keep the UI provider-neutral; keep the backend trait boundary narrow.
- Prefer root-cause changes in Rust/store/DTO layers over Svelte-side data reconstruction.
- Reuse existing helpers and dependencies; minimize new files and abstractions.
- Add one runnable regression check for each parser, query, race, or trust-boundary fix.
- Preserve unrelated dirty-worktree changes. Do not commit unless explicitly requested.
- Never log or expose credentials/tokens anywhere in the wiki.

## Growth

At this scale [[index]] is enough. If the wiki passes a few hundred pages, add a local
markdown search engine (for example qmd) as a CLI/MCP tool. Do not add embedding RAG
infrastructure before the index stops working.
