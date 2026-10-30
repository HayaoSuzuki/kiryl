---
type: Decision
title: Documentation ownership
description: Separate user guidance from development knowledge and keep one current
  owner per topic.
status: draft
sources:
- id: agents
  resource: ../../../AGENTS.md
  working_tree: untracked
  sha256: 13e7bdc0aa31c2f8b933edc75dd6947e091804c631597c183b99d427630db450
- id: procedure
  resource: ../documentation.md
  working_tree: untracked
  sha256: 57910cdbaf0a862f6b967cefa6ee4564a07e04dee36ad3232a51eca76e6b7208
- id: site
  resource: ../../../mkdocs.yml
  working_tree: untracked
  sha256: 7273b079d389a7c2bb8aa4a11375c817e5f69ee06bfa35d4364297b04250d197
- id: checker
  resource: ../../../tools/check_docs.py
  working_tree: untracked
  sha256: 725c44b4cceed6394f317d88a42c45818e4839743868983a1bce15aaf6152dee
---
# Decision and scope

Keep user instructions in `docs/user/`. Keep contributor procedures and the
AI development knowledge catalog in `docs/development/`. Start development
through `AGENTS.md` and the task-oriented OKF index. The user site reads only
the user tree.[^agents][^site]

The repository owner requested this separation after finding hoimin's combined
documentation difficult to navigate. Use one current owner per topic: guides
own procedures, and OKF concepts own the reasons, conditions, and evidence for
decisions and contracts. Follow the [maintenance procedure](../documentation.md)
instead of repeating its commands here.[^procedure]

# Alternatives and consequences

A shared tree with separate navigation would require a publication allowlist
and allow development context to enter search when the allowlist drifts.
Separate trees give the site a direct content boundary. This adds a second
entry point, so the root README links to both audiences.[^site][^procedure]

A separate ADR collection beside an OKF summary would create two current
explanations of each decision. Store the decision in OKF directly and cite
necessary original evidence. Dated work records remain supporting material;
developers read the current concept first.[^agents][^procedure]

# Limits and reconsideration

This draft records the chosen repository organization, not a reviewed product
architecture. The checks detect selected structural and navigation errors;
developers still review factual claims and user instructions. Revisit the
ownership boundaries if one topic acquires duplicate current explanations or
readers need historical reports to complete ordinary tasks.[^procedure][^checker]

[^agents]: [Agent entry point and ownership rules](../../../AGENTS.md).
[^procedure]: [Documentation maintenance](../documentation.md).
[^site]: [User-site build configuration](../../../mkdocs.yml).
[^checker]: [Documentation validator](../../../tools/check_docs.py).
