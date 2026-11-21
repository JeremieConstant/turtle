# SQLite is the only database, with no abstraction for others

All data is stored in embedded SQLite. With one node and one Workspace (ADR-0002), an external database would add an operational dependency without buying anything, and a backup stays a file copy. We deliberately do not build a storage abstraction "so Postgres can be added later": it would cost effort on every query for a case we have ruled out.

## Consequences

SQLite allows one writer at a time, so live editing must not write to the database on every keystroke; document state is held in memory and persisted in batches. The target of 500 people working at the same time held: the load test (`scripts/load-test.sh`) had 500 people type on 100 Pages on one node, and every change reached the others in milliseconds while saving used a fraction of the save interval. `docs/load-test.md` records the run and its hardware.
