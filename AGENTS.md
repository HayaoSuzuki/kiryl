# Developing kiryl

Start with [the developer entry point](docs/development/index.md), then use the
[OKF index](docs/development/knowledge/index.md) to select the contracts and
decisions relevant to your task. Follow their sources into current code and
tests before making changes. Read only relevant topics; the catalog is not a
requirement to load the entire documentation tree.

## Documentation ownership

- Put installation, tutorials, CLI usage, and troubleshooting for users in
  `docs/user/`. Describe implemented behavior and tested prerequisites.
- Put development procedures in `docs/development/`. Record design decisions
  and contracts in its `knowledge/` OKF bundle, with reasons, sources, scope,
  and conditions that require reconsideration.
- Update the existing owner of a topic. Link to commands and specifications
  instead of copying them into multiple documents. Keep indexes short.
- Keep temporary investigation and progress notes outside maintained guides.
  If a workflow requires durable plans or reports, use
  `docs/development/work/<issue-or-topic>/` and link relevant evidence from the
  current concept. Create a directory only when it contains useful evidence.
- Update user guidance when behavior changes; update relevant contracts and
  decisions in the same change. If neither changes, explain why no OKF update
  was needed in the PR.

Follow [the documentation procedure](docs/development/documentation.md) for
OKF metadata, source attribution, review, and checks. Treat earlier test results
as evidence about their recorded revision and environment. Record disagreements
between documentation and implementation before resolving them.

## Project scope

Implementation is primarily Rust and licensed under Elastic-2.0. The repository
currently contains a development scaffold. Engine selection remains open;
do not introduce Atheris or another engine as an established dependency without
the corresponding experiment and decision. Deployment and publishing are deferred.
