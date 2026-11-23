# Triage Labels

The skills speak in terms of five canonical triage roles. This file maps those roles to the actual label strings used in this repo's issue tracker.

| Label in mattpocock/skills | Label in our tracker | Meaning                                  |
| -------------------------- | -------------------- | ---------------------------------------- |
| `needs-triage`             | `needs-triage`       | Maintainer needs to evaluate this issue  |
| `needs-info`               | `needs-info`         | Waiting on reporter for more information |
| `ready-for-agent`          | `ready-for-agent`    | Fully specified, ready for an AFK agent  |
| `ready-for-human`          | `ready-for-human`    | Requires human implementation            |
| `wontfix`                  | `wontfix`            | Will not be actioned                     |

An issue is `ready-for-agent` only when every choice that binds other work is made in it: a format another issue reads, a name in the API, what a person sees. Such a choice still open makes it `needs-info`, with the open choices listed; the agent would otherwise make them alone and unseen. Issue #23 left the export's layout open, which the import depends on (ADR-0011).

Nobody is there to answer while a `ready-for-agent` issue is worked on. Where a skill asks to confirm something with the user first, such as the seams to test at, the agent decides, states its choice, and carries on.

When a skill mentions a role (e.g. "apply the AFK-ready triage label"), use the corresponding label string from this table.

Edit the "Label in our tracker" column to match whatever vocabulary you actually use.
