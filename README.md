# weekend-db

A personal weekend project: a portable C embedded database with persistent maps
and lists, durable snapshots, and conservative concurrent record commits. Rust
is the first client language. This directory currently contains the design and
learning plan; the user will write the implementation.

Start with [DESIGN.md](DESIGN.md): one coherent target design with data-structure
sketches, transaction and recovery logic, decision rationale, and references.
Then use the [roadmap](ROADMAP.md), including its first-weekend checklist.

- [Agreed scope and acceptance cases](PLAN.md)
- [Domain vocabulary](GLOSSARY.md)
- [Current transaction decision](docs/adr/0007-merge-stable-record-assignments.md)
- [Transaction-model research and tradeoffs](docs/design/transaction-model-research.md)
- [Comparison with existing solutions and potential niche](docs/design/positioning-research.md)
- [Schema and deferred list-rebasing notes](docs/design/list-rebasing-and-record-schemas.md)

Plan for roughly 6–8 hours per weekend. Milestones are learning checkpoints,
not a delivery schedule or a claim of production safety. Earlier ADRs marked
superseded preserve the rationale for narrowing the project.
