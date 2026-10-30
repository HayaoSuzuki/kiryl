# Maintaining documentation

Use [the user guide](../user/index.md) to describe how to use kiryl. Use the
[development entry point](index.md) to work on kiryl. Current guides use English,
following hoimin; preserve the original language of quoted evidence.

## Choose one owner for each topic

| Topic | Owner | Content |
| --- | --- | --- |
| Installation, tutorials, commands, troubleshooting | `docs/user/` | Implemented user workflows and their prerequisites |
| Contributor setup and validation commands | `docs/development/index.md` | How to build and check changes |
| Documentation procedure | This page | Where to write, update, and validate documentation |
| Current decisions and contracts | `docs/development/knowledge/` | Reasons, alternatives, conditions, sources, and limits |
| Necessary original plans or evidence | `docs/development/work/<issue-or-topic>/` | Task-specific records linked from current concepts |

Update a topic's existing page instead of adding a dated successor containing
another current explanation. Keep one independently retrievable topic per
concept. Split a page when readers need its topics for different tasks; avoid
building a directory tree before there is content for it. Keep temporary notes
outside maintained documentation. Retain durable evidence only when a future
developer needs it to assess a decision or guarantee.

Keep current commands in their procedural guide, and link to that guide from
OKF concepts. A decision explains why; a user guide explains the operation and
expected result. Readers should not need historical reports to follow a user
workflow.

## Read and update OKF

Start at [the OKF index](knowledge/index.md), select relevant concepts, and
inspect their implementation, tests, and source documents. Record contradictions
before changing the contract or code. Update affected concepts and user guidance
in the same change. Typographical corrections and changes that leave contracts,
decisions, and evidence unchanged need no additional concept.

The bundle uses [OKF v0.2](https://github.com/GoogleCloudPlatform/knowledge-catalog/blob/main/okf/SPEC.md).
Its boundary is `docs/development/knowledge/`; ordinary guides, plans, and source
code remain outside that boundary.

- Ordinary concepts start with YAML frontmatter and a nonempty string `type`.
  Local types include `Decision`, `Contract`, `Audit`, and `Reference`; they are
  conventions, and other types and metadata are allowed.
- Give new concepts a title, description, and `status: draft`. Record the
  decision or contract, rationale, evidence, limitations, and reconsideration
  triggers. Describe proposals and implemented behavior separately.
- Use `sources` entries with distinct `id` and concrete `resource` values.
  Cite sources beside the claims they support using matching `[^id]` footnotes
  and definitions. Use relative links so readers can follow them on GitHub.
- Record the actual revision read for committed sources. For uncommitted
  sources, record `working_tree: modified` or `untracked` and the SHA-256 of
  the bytes read. Do not invent a future commit. A recorded hash describes a
  source snapshot, rather than a promise that the source will never change.
- Use `verified` only for a real content review with an identifiable actor and
  time. Structural validation alone does not verify a claim. Keep historical
  revisions, platforms, and execution limits attached to their evidence.
- The bundle-root `index.md` declares only `okf_version: "0.2"` in its
  frontmatter. Nested `index.md` and `log.md` files have no frontmatter. Link
  every concept from a relevant index; add nested indexes only when useful.

Record alternatives and the reason for choosing an approach in the owning
decision. Revisit that decision when its assumptions change. Update the existing
decision and explain a superseded choice there; use Git history for old text.

## Validate and preview

Use Python 3.14 and uv. From the repository root:

```sh
uv sync --frozen --only-group docs --no-install-project
uv run --frozen --no-sync pytest
uv run --frozen --no-sync python -m tools.check_docs
uv run --frozen --no-sync mkdocs build --strict
```

The checker parses YAML, checks reserved files and source-footnote associations,
and rejects missing local references and concepts unreachable from the OKF
index. These include repository maintenance rules stricter than OKF itself.
Unknown concept types and metadata are accepted. Broken references are
maintenance failures, not an assertion that OKF forbids broken links.

The reference scanner supports inline links, reference links, and footnotes
outside fenced examples. It does not implement every Markdown extension or
validate raw HTML links, developer-page heading fragments, external URL
availability, source hash freshness, or factual claims. Review those separately.
MkDocs checks user-page navigation and heading fragments during the strict build.

For a machine-readable OKF reference inventory:

```sh
uv run --frozen --no-sync python -m tools.knowledge_references
```

For a local user-guide preview:

```sh
uv run --frozen --no-sync mkdocs serve
```

MkDocs reads only `docs/user/`. Developer guides, OKF, and work records remain
outside the generated site's navigation, HTML, and search index. Do not copy
them into the user tree to resolve a link. Link to an appropriate repository
page when users need contributor information.

The Documentation CI workflow checks the repository and builds an HTML artifact
for review. It does not deploy a site. Preview output lives in ignored `site/`.
Local documentation hooks run after installing the docs dependency group and
enabling prek as described in the [development guide](index.md).

## Source of the imported mechanisms

The source was hoimin at `542ec59c002d72fdff0137aa90113ffa59d70844`:

- [OKF procedure](https://github.com/HayaoSuzuki/hoimin/blob/542ec59c002d72fdff0137aa90113ffa59d70844/docs/okf-workflow.md)
- [Documentation build and publication](https://github.com/HayaoSuzuki/hoimin/blob/542ec59c002d72fdff0137aa90113ffa59d70844/docs/documentation.md)
- [Reference inventory implementation](https://github.com/HayaoSuzuki/hoimin/blob/542ec59c002d72fdff0137aa90113ffa59d70844/tools/knowledge_references.py)

The inventory and its behavioral tests are reused under Elastic-2.0. kiryl adds
structure and citation checks, separates document trees by audience, and builds
the user tree directly. It does not copy hoimin's publication helper, historical
catalogs, or deployment workflow.
