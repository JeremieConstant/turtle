# Contributing to Turtle

Thank you for wanting to help. This file is everything you need to know before opening a pull request.

Everyone taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md). A vulnerability is reported privately, as the [security policy](SECURITY.md) describes, never in a public issue.

## What is welcome

Pull requests for issues labelled [`good first issue`](https://github.com/turtle-wiki/turtle/labels/good%20first%20issue) or [`help wanted`](https://github.com/turtle-wiki/turtle/labels/help%20wanted) are welcome as they are.

Anything larger starts with an issue, so we can agree on it before you build something that will not be merged. Questions go to [GitHub Discussions](https://github.com/turtle-wiki/turtle/discussions).

## How long it takes

Turtle is maintained in a few hours a week, so a review can take a few days.

## Sign off your commits

Every commit is signed off under the [Developer Certificate of Origin](https://developercertificate.org):

```bash
git commit -s
```

This adds a line `Signed-off-by: Your Name <you@example.com>` with the name and email of the commit's author. With it you confirm that you wrote the change, or otherwise have the right to submit it, under the project's license, and that the contribution and your sign-off are kept as a public record.

There is no CLA: your contribution stays yours, licensed under [AGPL-3.0](LICENSE) like the rest of Turtle.

A check on every pull request names any commit without a sign-off and says how to fix it. To sign off the last commit:

```bash
git commit --amend -s --no-edit
```

To sign off every commit on your branch, rebase onto the commit it started from, which the check prints:

```bash
git rebase --signoff <commit the branch started from>
```

Then push again with `git push --force-with-lease`.

## When a change is done

`scripts/check.sh` passes: it runs everything CI runs. Enable the pre-commit hook once per clone and it runs before each commit:

```bash
git config core.hooksPath .githooks
```

Run cargo as `scripts/cargo`, which finds it when it is not on your `PATH`.

Review also reads the code against [CODING_STANDARDS.md](CODING_STANDARDS.md). Names follow the terms in [GLOSSARY.md](GLOSSARY.md), and the decisions behind Turtle are recorded in [docs/adr/](docs/adr/).

## Commits

A commit that finishes an issue ends its message with `Closes #N`. A commit towards an issue that is not finished yet uses `Refs #N`. A bare `(#N)` in the title does not close the issue.

## AI-assisted contributions

Contributions written with AI are welcome on the same terms: the checks pass, the commits are signed off by you, and the pull request explains in your own words what it changes and why.

## Run Turtle locally

```bash
scripts/preview.sh
```

builds Turtle and runs it at <http://127.0.0.1:4100>. In a second terminal,

```bash
scripts/seed.sh
```

gives the fresh Turtle an account, Areas and Pages, and prints the address of a Page. The email and password are in `seed-account.txt` in the data directory, `turtle-data-dev`. Both scripts take `TURTLE_PORT` and `TURTLE_DATA` to run a second Turtle beside the first.
