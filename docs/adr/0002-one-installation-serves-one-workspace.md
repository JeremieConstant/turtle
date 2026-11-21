# One installation serves exactly one Workspace, on a single node

An installation holds one Workspace and runs on one node, aimed at teams of up to about 500 people. We left out multi-tenancy and clustering on purpose: they complicate permissions, search and data isolation, and are only needed to run a hosted service, which is not the goal. This is what allows a single binary with embedded components instead of external services.
