# Code-Qualität: Was Kaneo prüft und was Turtle übernehmen sollte

Stand: 25.11.2026. Kaneo gelesen auf Commit [`e46680d`](https://github.com/usekaneo/kaneo/tree/e46680d28a00be1173922ad065ca69243e30ab31) (`main`). Alle Kaneo-Links unten sind auf diesen Commit festgelegt. Die Check-Runs stammen von Kaneos zuletzt gemergtem PR #2247 (Head `a00aecb`), abgefragt mit `gh api repos/usekaneo/kaneo/commits/<sha>/check-runs`.

## 1. Kurzfassung

- Kaneo setzt auf **ein Werkzeug für Format und Lint** ([Vite+](https://viteplus.dev/guide/check), also Oxlint und Oxfmt, laut Kommentar in der Konfiguration Nachfolger einer Biome-Konfiguration), das per Husky **vor jedem Commit** und in CI läuft. Dazu kommen Typecheck, Unit-, Integrations-, Storage- und Browser-Tests sowie Docker-Builds bei **jedem PR**.
- Viele Prüfungen gelten der **Lieferkette und den Workflows selbst**: Renovate (wöchentlich, 7 Tage Mindestalter, Majors nur nach Freigabe), Socket Security als GitHub-App, TruffleHog-Secret-Scan je PR und nachts über die ganze Historie, actionlint, shellcheck und zizmor, alle Actions per SHA gepinnt.
- Ein großer Teil von Kaneos `.github/` ist **Community-Moderation** für ein Projekt mit vielen Fremd-PRs: Conventional-Commit-Titel, semantic-release, PR-Größen-Labels, KI-/„Slop“-Erkennung, Berechtigungsprüfung für Beitragende, eigene Review-Bots. Für Turtle mit einer Person ist das zu viel.
- Turtle hat den Kern schon: `scripts/check.sh` (Clippy mit `-D warnings`, Drift-Check der generierten Typen, svelte-check, Vitest, `cargo test`) im Pre-commit-Hook und in CI, dazu Branch-Protection mit Pflicht-Check `check`.
- Bei Turtle fehlen fast nur Prüfungen, die **ohne Codeänderung neue Probleme finden**: Laut GitHub-API sind Secret Scanning, Push Protection und Dependabot Security Updates für `turtle-wiki/turtle` **ausgeschaltet**, Abhängigkeits-Updates gibt es nicht, ebenso keinen Advisory-/Lizenz-Check für Cargo.
- **Hoch**: (1) Secret Scanning und Push Protection und Dependabot-Alerts einschalten (gratis, ein Klick), (2) Dependabot für `cargo`, `npm`, `github-actions`, `docker`, (3) `cargo-deny` (Advisories und Lizenzen) in CI, auch wöchentlich nach Zeitplan.
- **Mittel**: `cargo fmt --check` (einmal 59 von 63 Rust-Dateien umformatieren), Workflow-Härtung (`permissions`, SHA-Pins, actionlint/zizmor), Docker-Build auch im PR, strengere `tsconfig`-Flags, Prettier mit Svelte-Plugin.
- **Nicht übernehmen**: commitlint/PR-Titel-Check, semantic-release, KI-Review-Bots, Moderations-Workflows, Coverage-Gates (Kaneo hat selbst keins), Renovate statt Dependabot, eigene TruffleHog-Installation.

## 2. Was Kaneo einsetzt

`K/` steht für `https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/`. Die Links stehen ausgeschrieben unter Quellen.

| Check | Tool | Wann | Quelle |
|---|---|---|---|
| Format und Lint (JS/TS/React) | Vite+ `vp check` (Oxfmt und Oxlint mit Plugins `typescript`, `unicorn`, `oxc`, `react`, `jsx-a11y`, Kategorie `correctness` als Fehler; type-aware aus) | Pre-commit (Husky), PR, Push `main` | [K/.husky/pre-commit][k-precommit], [K/vite.config.ts][k-vite], [K/package.json][k-pkg] (`"lint": "vp check"`), Job `quality` in [K/.github/workflows/ci.yml][k-ci] |
| Commit-Nachrichten | commitlint, `@commitlint/config-conventional` | commit-msg-Hook | [K/.husky/commit-msg][k-commitmsg], [K/commitlint.config.js][k-commitlint] |
| PR-Titel im Conventional-Commit-Format | commitlint gegen den Titel, mit Konfiguration vom Base-Branch | PR (opened/edited/synchronize …) | [K/.github/workflows/pr-title.yml][k-prtitle] |
| Typecheck | `tsc` je Paket (`vp run -r typecheck`), `strict`, `noUnusedLocals`, `noUnusedParameters`, `noFallthroughCasesInSwitch` | PR, Push `main` | Job `quality` in [ci.yml][k-ci], [K/apps/web/tsconfig.app.json][k-tsweb] |
| Übersetzungen vollständig | eigenes Skript `i18n:check` gegen `en-US` | PR, Push | Job `quality` in [ci.yml][k-ci], [package.json][k-pkg] |
| API-Referenz passt zu den Routen | eigenes Skript `openapi:check` | PR, Push | Job `quality` in [ci.yml][k-ci] |
| Unit-Tests | Vitest (`vp run -r test`), dazu `node --test` für Sicherheits-/CI-Skripte | PR, Push | Job `unit` in [ci.yml][k-ci] |
| Integrations-Tests | Vitest gegen Postgres-Service, 4 Shards | PR, Push (entfällt bei als „billig“ klassifizierten PRs) | Job `integration` in [ci.yml][k-ci] |
| Storage-Tests | Vitest gegen echtes MinIO/S3 | PR, Push | Job `storage` in [ci.yml][k-ci] |
| Build aller Pakete, Links/SEO der Website | `pnpm build`, `seo:check` | PR, Push | Job `build` in [ci.yml][k-ci] |
| Browser-Smoke-Tests | Playwright (Chromium) gegen frisch gebautes Image | PR | Job `smoke` in [ci.yml][k-ci] |
| Image-Build und Laufzeittests | Docker-Build (`push: false`), Browser-Regression, Realtime mit/ohne Redis, **Upgrade von der letzten stabilen Version** | PR, Push | Jobs `docker-build`, `split-images` in [ci.yml][k-ci] |
| Workflow-Lint | actionlint (mit Prüfsumme geladen), shellcheck | PR, Push | Job `workflows` in [ci.yml][k-ci], [K/.github/actionlint.yaml][k-actionlint] |
| Workflow-Sicherheit | zizmor; blockiert nur „High/High“, Rest als SARIF in Code Scanning; Policy `hash-pin` für alle `uses:` | PR, Push, nachts | Job `workflows` in [ci.yml][k-ci], [K/.github/workflows/nightly.yml][k-nightly], [K/zizmor.yml][k-zizmor] |
| Secret-Scan | TruffleHog (Diff je PR/Push, volle Historie nachts), SARIF an Code Scanning, Discord-Hinweis | PR, Push, nachts | Job `secret-scan` in [ci.yml][k-ci], [nightly.yml][k-nightly], [K/.trufflehog-exclude][k-truffle] |
| Ein Pflicht-Check für alle Jobs | Gate-Job `ci-result` lehnt fehlgeschlagene und unerwartet übersprungene Jobs ab | PR, Push | Job `ci-result` in [ci.yml][k-ci] |
| Teure Jobs überspringen | `detect-pr-changes` klassifiziert den Diff | PR | Job `detect-pr-changes` in [ci.yml][k-ci] |
| Abhängigkeits-Updates | Renovate: `npm`, `github-actions`, `dockerfile`; montags vor 6 Uhr; max. 3 offene PRs; `minimumReleaseAge: 7 days` (npm); Gruppen (Tiptap, React …); Majors nur nach Freigabe im Dashboard; Sicherheits-Updates sofort; `helpers:pinGitHubActionDigests` | Zeitplan | [K/renovate.json][k-renovate] |
| Abhängigkeits-Risiko im PR | Socket Security (GitHub-App; Checks „Project Report“, „Pull Request Alerts“) | PR | Check-Runs von PR #2247 (App `socket-security`) |
| Dependency Graph | eigene Snapshot-Übermittlung an GitHub | Push `main` | [K/.github/workflows/dependency-graph.yml][k-depgraph] |
| Release und Changelog | semantic-release (Conventional Commits, Changelog, GitHub-Release), wartet auf grüne CI desselben Commits | manuell (`workflow_dispatch`) | [K/release.config.js][k-release-cfg], [K/.github/workflows/release.yml][k-release] |
| Nightly | ruft `ci.yml` erneut auf und veröffentlicht `nightly`-Images | nachts | [nightly.yml][k-nightly] |
| KI-Code-Review | CodeRabbit (Auto-Review aus), eigener Bot „Peekareview“/„Peekareq“ | auf Anfrage | [K/.coderabbit.yaml][k-coderabbit], [K/.github/workflows/code-review.yml][k-codereview], [K/.github/workflows/ui-review.yml][k-uireview] |
| PR-Moderation | AgentScan, „AI comment labeler“, PR-Größen-Labels, Berechtigungsprüfung, Moderation „mellon“, Auto-Assign | PR (`pull_request_target`), teils stündlich | [agentscan.yml][k-agentscan], [slop-labeler.yml][k-slop], [pr-size-labeler.yml][k-prsize], [contribution-eligibility.yml][k-elig], [mellon.yml][k-mellon], [auto-assign.yml][k-assign] |
| PR-Vorlage | „What changed? / Related issue / How did you check it?“ | PR | [K/.github/PULL_REQUEST_TEMPLATE.md][k-prtemplate] |

Was Kaneo **nicht** hat: keinen Coverage-Upload und kein Coverage-Gate (kein solcher Schritt in [ci.yml][k-ci]), kein CodeQL-Codeanalyse-Workflow (Code Scanning bekommt nur SARIF von TruffleHog und zizmor), keine `dependabot.yml` (Renovate stattdessen), kein CODEOWNERS.

Durchgängig in allen Workflows: `permissions: contents: read` als Vorgabe, `persist-credentials: false`, `timeout-minutes` je Job, `concurrency` mit `cancel-in-progress` außerhalb von `main`, alle Actions per Commit-SHA gepinnt (z. B. [ci.yml][k-ci]).

## 3. Was Turtle schon hat

| Check | Tool | Wann | Quelle |
|---|---|---|---|
| Rust-Lints | Clippy, `--all-targets -- -D warnings` | Pre-commit, PR, Push `main` | [`scripts/check.sh`](../../scripts/check.sh), [`.githooks/pre-commit`](../../.githooks/pre-commit), [`.github/workflows/check.yml`](../../.github/workflows/check.yml) |
| Generierte Operation-Typen passen zum Katalog | `npm run generate` und Vergleich mit `shasum` | Pre-commit, PR, Push | [`scripts/check.sh`](../../scripts/check.sh) |
| Typecheck der Web-App | `svelte-check` mit `strict: true`, `verbatimModuleSyntax` | Pre-commit, PR, Push | [`web/package.json`](../../web/package.json), [`web/tsconfig.json`](../../web/tsconfig.json) |
| Web-Tests | Vitest | Pre-commit, PR, Push | [`scripts/check.sh`](../../scripts/check.sh) |
| Server-Tests | `cargo test` | Pre-commit, PR, Push | [`scripts/check.sh`](../../scripts/check.sh) |
| Pflicht-Check vor dem Merge | Branch-Protection auf `main`: Status-Check `check` Pflicht, offene Konversationen müssen gelöst sein, kein Force-Push | PR | `gh api repos/turtle-wiki/turtle/branches/main/protection` |
| Container-Image | Multi-Arch-Build (amd64/arm64) und Push nach GHCR | Push `main`, Tags `v*.*.*` (nicht im PR) | [`.github/workflows/image.yml`](../../.github/workflows/image.yml) |
| Review-Standards für Menschen und Agents | Urteilsfragen, die kein Tool entscheidet | Review | [`CODING_STANDARDS.md`](../../CODING_STANDARDS.md), [`AGENTS.md`](../../AGENTS.md) |

Was laut GitHub-API (`gh api repos/turtle-wiki/turtle --jq .security_and_analysis`) **aus** ist: `secret_scanning`, `secret_scanning_push_protection`, `dependabot_security_updates`. Es gibt keinen Rust-Formatter-Check (`cargo fmt --check` meldet heute Abweichungen in 59 von 63 `.rs`-Dateien), keinen Web-Formatter oder -Linter außer svelte-check, keine Abhängigkeits-Updates und keinen Advisory- oder Lizenz-Check. `check.yml` hat keinen `permissions`-Block, Actions sind per Tag gepinnt (`@v4`), nicht per SHA.

## 4. Empfehlungen für Turtle

Maßstab: billig in Laufzeit und Pflege, für 5–10 h/Woche. Was ohne Netz läuft und schnell ist, gehört in `scripts/check.sh` (Definition of Done). Was Netz braucht oder sich ohne Codeänderung ändern kann (Advisories), läuft als eigener CI-Job, zusätzlich nach Zeitplan.

Branch-Protection kennt nur den Check `check`. Neue Jobs entweder als Schritte in denselben Job `check` legen oder in der Branch-Protection als Pflicht eintragen. Kaneo löst das mit einem Gate-Job (`ci-result` in [ci.yml][k-ci]), was sich bei Turtle erst ab mehreren Jobs lohnt.

### Hoch

**H1. Secret Scanning, Push Protection und Dependabot-Alerts einschalten**
- *Was*: In Settings → Advanced Security: Secret Scanning, Push Protection, Dependabot alerts und Dependabot security updates.
- *Warum*: Für öffentliche Repos läuft Secret Scanning kostenlos ([GitHub-Doku Secret Scanning][gh-secret]). Push Protection blockiert den Push, bevor ein Geheimnis in der Historie landet ([GitHub-Doku Push Protection][gh-push]). Security Updates öffnen PRs nur für Abhängigkeiten mit Advisory ([GitHub-Doku Security Updates][gh-secupd]). Kaneo erreicht dasselbe mit einer selbst installierten TruffleHog-Version je PR und nachts ([ci.yml][k-ci], [nightly.yml][k-nightly]); bei Turtle erledigt GitHub das ohne Workflow. `turtle-data*/` ist zwar in `.gitignore`, aber SMTP-Zugangsdaten oder Tokens in Skripten, Tests oder `compose.yml` fängt nur ein Scanner.
- *Aufwand*: 5 Minuten, keine Pflege. Ist eine Einstellung im Repo, die der Maintainer selbst setzt.

**H2. Dependabot-Version-Updates für Cargo, npm, Actions und Docker**
- *Was*: `.github/dependabot.yml`, wöchentlich, gruppiert, mit Cooldown.
- *Warum*: Kaneo hält Abhängigkeiten mit Renovate aktuell: wöchentlich, max. 3 offene PRs, 7 Tage Mindestalter, Gruppen für zusammengehörige Pakete wie `@tiptap/*`, Majors nur nach Freigabe ([renovate.json][k-renovate]). Dependabot kann das Wichtigste davon ohne Drittanbieter-App: `groups`, `cooldown`, `open-pull-requests-limit`, `schedule` ([Dependabot options reference][gh-dbopts]). Mindestens 3 Tage Cooldown wendet GitHub laut Doku sogar ohne Konfiguration an. Turtle nutzt Tiptap und Yjs, die sich oft gemeinsam ändern, deshalb die Gruppe. Der Check-Job prüft jeden Dependabot-PR mit derselben `check.sh`.
- *Aufwand*: eine Datei; danach etwa 1–4 PRs pro Woche durchsehen und mergen.
- *Skizze*:
  ```yaml
  # .github/dependabot.yml
  version: 2
  updates:
    - package-ecosystem: cargo
      directory: /server
      schedule: { interval: weekly, day: monday }
      cooldown: { default-days: 7 }
      open-pull-requests-limit: 3
      groups:
        rust-minor-patch: { update-types: [minor, patch] }
    - package-ecosystem: npm
      directory: /web
      schedule: { interval: weekly, day: monday }
      cooldown: { default-days: 7 }
      open-pull-requests-limit: 3
      groups:
        tiptap: { patterns: ["@tiptap/*"] }
        web-minor-patch: { update-types: [minor, patch] }
    - package-ecosystem: github-actions
      directory: /
      schedule: { interval: monthly }
      groups:
        actions: { patterns: ["*"] }
    - package-ecosystem: docker
      directory: /
      schedule: { interval: monthly }
  ```
  Commit-Nachrichten der PRs folgen nicht der Turtle-Konvention mit `Closes #N`, was nicht stört, da sie keine Issue schließen.

**H3. `cargo-deny`: Advisories, Lizenzen, Quellen**
- *Was*: `cargo deny check` mit einer `server/deny.toml` als eigener CI-Job, bei PRs und wöchentlich per `schedule`.
- *Warum*: cargo-deny prüft die RustSec-Advisory-Datenbank, erlaubte Lizenzen, doppelte oder verbotene Crates und erlaubte Registries ([cargo-deny-Doku][deny]). Für ein AGPL-Projekt ist der Lizenz-Check der eigentliche Gewinn: eine neue transitive Abhängigkeit mit inkompatibler Lizenz fällt im PR auf statt beim Release. Der Zeitplan fängt Advisories zu Crates, die sich nicht geändert haben. Kaneo deckt diese Lücke für npm mit Socket Security und Renovates `vulnerabilityAlerts` ab (Check-Runs von PR #2247, [renovate.json][k-renovate]); für Rust ist cargo-deny das übliche Werkzeug. `cargo audit` ([cargo-audit][audit]) deckt nur Advisories ab, cargo-deny zusätzlich Lizenzen, deshalb cargo-deny.
- *Aufwand*: einmal `deny.toml` anlegen und die Lizenzliste an die heutigen Abhängigkeiten anpassen (`cargo deny init`, dann `cargo deny check licenses` und Funde abarbeiten), etwa 1 h. Nicht in `check.sh`, weil Advisories Netz brauchen und sich ohne Codeänderung ändern; sonst würde der Pre-commit-Hook plötzlich rot.
- *Skizze*:
  ```toml
  # server/deny.toml (Ausgangspunkt, Liste aus `cargo deny check licenses` ergänzen)
  [advisories]
  version = 2
  yanked = "deny"

  [licenses]
  version = 2
  allow = ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-2-Clause",
           "BSD-3-Clause", "ISC", "Unicode-3.0", "Zlib", "MPL-2.0", "AGPL-3.0-only"]

  [bans]
  multiple-versions = "warn"   # doppelte Versionen nur melden

  [sources]
  unknown-registry = "deny"
  unknown-git = "deny"
  ```
  ```yaml
  # .github/workflows/deny.yml
  name: Deny
  on:
    pull_request:
      paths: ["server/Cargo.toml", "server/Cargo.lock", "server/deny.toml"]
    push: { branches: [main] }
    schedule: [{ cron: "23 4 * * 1" }]
  permissions: { contents: read }
  jobs:
    deny:
      runs-on: ubuntu-latest
      timeout-minutes: 10
      steps:
        - uses: actions/checkout@v4
          with: { persist-credentials: false }
        - uses: EmbarkStudios/cargo-deny-action@v2
          with: { manifest-path: server/Cargo.toml }
  ```
  Fürs Web genügen die Dependabot-Alerts aus H1, weil npm-Advisories dort schon auftauchen.

### Mittel

**M1. `cargo fmt --check` in `check.sh`**
- *Was*: Einmal `cargo fmt` über den Server laufen lassen, den Commit in `.git-blame-ignore-revs` eintragen, dann `cargo fmt --check` als ersten Schritt in `check.sh`.
- *Warum*: Kaneo erzwingt Formatierung vor jedem Commit und in CI ([.husky/pre-commit][k-precommit], `vp check` laut [Vite+-Doku][vplus] Format plus Lint). Für Rust ist rustfmt der Standard und hat genau dafür `--check` ([rustfmt][rustfmt]). Ein festes Format nimmt Formatfragen aus Reviews heraus, die `CODING_STANDARDS.md` ausdrücklich Tools überlässt („anything a tool can decide is in `scripts/check.sh`“). Das gilt auch für Code, den Agents schreiben.
- *Aufwand*: Der eine Umformatierungs-Commit ist groß (59 von 63 Dateien) und erzeugt Konflikte in offenen Branches, deshalb nach dem Mergen offener PRs. Danach null Pflege, der Check dauert unter einer Sekunde. Wer den aktuellen Stil halten will, legt Abweichungen in `server/rustfmt.toml` fest, statt den Check wegzulassen.
- *Skizze*:
  ```bash
  # scripts/check.sh, vor "Server lints"
  step "Server formatting"
  cargo fmt --manifest-path server/Cargo.toml --check
  ```
  In `check.yml`: `components: clippy, rustfmt`.

**M2. Workflows härten: Rechte, Pins, actionlint und zizmor**
- *Was*: `permissions: contents: read` oben in `check.yml`, `persist-credentials: false` beim Checkout, `timeout-minutes`, `concurrency` mit `cancel-in-progress` für PRs; Actions per SHA pinnen (Dependabot aus H2 aktualisiert die SHAs); actionlint und zizmor als Schritt.
- *Warum*: Kaneo macht all das in jedem Workflow und blockiert nur hochsichere zizmor-Funde ([ci.yml][k-ci], [zizmor.yml][k-zizmor]). GitHub empfiehlt minimale `GITHUB_TOKEN`-Rechte und SHA-Pins als einzige unveränderliche Referenz ([Security hardening for GitHub Actions][gh-harden]). actionlint findet Syntax- und Ausdrucksfehler, bevor ein Workflow erst auf `main` scheitert ([actionlint][actionlint]); zizmor findet Muster wie Template-Injection oder zu weite Rechte ([zizmor][zizmor]). `image.yml` hat `packages: write` und läuft auf `main`; genau dort lohnt sich der Blick.
- *Aufwand*: einmal etwa 1 h; zizmor und actionlint laufen in Sekunden. Lohnt sich, solange Turtle nur zwei, drei Workflows hat und Fehler billig zu beheben sind.
- *Skizze*:
  ```yaml
  # .github/workflows/check.yml (Kopf)
  permissions: { contents: read }
  concurrency:
    group: check-${{ github.ref }}
    cancel-in-progress: ${{ github.ref != 'refs/heads/main' }}
  jobs:
    check:
      timeout-minutes: 30
      steps:
        - uses: actions/checkout@<sha> # v4
          with: { persist-credentials: false }
        # …
    workflows:
      runs-on: ubuntu-latest
      timeout-minutes: 5
      steps:
        - uses: actions/checkout@<sha> # v4
          with: { persist-credentials: false }
        - uses: zizmorcore/zizmor-action@<sha>   # oder Binary mit Prüfsumme wie bei Kaneo
        - run: bash <(curl -s https://raw.githubusercontent.com/rhysd/actionlint/main/scripts/download-actionlint.bash) && ./actionlint
  ```
  Beim Download-Skript von actionlint lieber, wie Kaneo, eine feste Version mit Prüfsumme laden.

**M3. Image auch im PR bauen**
- *Was*: In `check.yml` (oder einem Job `image-build`) das Image für `linux/amd64` mit `push: false` bauen, Cache aus GHA wie in `image.yml`. Optional den Container starten und eine Seite abrufen.
- *Warum*: Heute baut `image.yml` nur nach dem Merge auf `main`; ein kaputtes `Dockerfile` oder ein Build-Schritt, den `check.sh` nicht berührt (`vite build`, statische Dateien, Fonts), fällt erst dann auf. Kaneo baut jedes Image im PR und testet sogar das Upgrade von der letzten stabilen Version ([ci.yml][k-ci], Jobs `docker-build`, `split-images`). Das Upgrade-Testing ist für Turtle mit SQLite-Migrationen interessant, aber erst wenn es Installationen gibt, die upgraden.
- *Aufwand*: Rust-Release-Build kostet CI-Minuten (ohne Cache mehrere Minuten), mit `cache-from: type=gha` deutlich weniger. Nur amd64, ARM bleibt in `image.yml`. Kann per `paths`-Filter auf Änderungen an `Dockerfile`, `server/**`, `web/**` begrenzt werden.

**M4. Strengere TypeScript-Flags**
- *Was*: In `web/tsconfig.json` `noUnusedLocals`, `noUnusedParameters`, `noFallthroughCasesInSwitch` ergänzen.
- *Warum*: Kaneo nutzt genau diese Flags zusätzlich zu `strict` ([apps/web/tsconfig.app.json][k-tsweb]) und erzwingt ungenutzte Variablen zusätzlich per Oxlint-Regel `no-unused-vars` ([vite.config.ts][k-vite]). svelte-check liest die `tsconfig.json`, Turtle bekommt die Prüfung also ohne neues Tool.
- *Aufwand*: einmal die Funde beheben, danach null Pflege.

**M5. Prettier mit `prettier-plugin-svelte` für `web/`**
- *Was*: Prettier und das Svelte-Plugin als devDependencies, `prettier --check src` in `check.sh`.
- *Warum*: Dasselbe Argument wie M1 für den Web-Teil. Biome nennt seine Svelte-Unterstützung selbst nur teilweise: Es formatiert und lintet nur die Skript-Blöcke und warnt, dass neuere Features und seltene Syntax noch nicht funktionieren ([Biome Language support][biome]). Kaneos Vite+ ist auf React zugeschnitten. Prettier mit dem offiziellen Plugin des Svelte-Teams deckt Markup, Skript und Stil ab ([prettier-plugin-svelte][pps]).
- *Aufwand*: ein großer Umformatierungs-Commit wie bei M1, danach null Pflege. Zwei neue devDependencies, die Dependabot pflegt.
- *Skizze*:
  ```json
  // web/package.json
  "scripts": { "format": "prettier --write src", "format:check": "prettier --check src" },
  "devDependencies": { "prettier": "^3", "prettier-plugin-svelte": "^3" }
  ```
  ```json
  // web/.prettierrc
  { "plugins": ["prettier-plugin-svelte"] }
  ```
  ```bash
  # scripts/check.sh
  step "Web app formatting"
  npm --prefix web run --silent format:check
  ```

### Niedrig

**N1. CodeQL Default Setup (Rust und JavaScript/TypeScript)**
- *Was*: In Settings → Code security „CodeQL analysis → Default“ einschalten, ohne eigenen Workflow.
- *Warum*: CodeQL unterstützt Rust und JavaScript/TypeScript ([CodeQL supported languages][codeql-lang]) und ist für öffentliche Repos kostenlos ([About code scanning with CodeQL][codeql]). Kaneo nutzt keine CodeQL-Analyse; deshalb gibt es hier keinen Erfahrungswert aus Kaneo. Turtles kritische Regeln („Hidden is not found“, Berechtigungen nur in `ops`) prüft CodeQL nicht, die bleiben beim Review.
- *Aufwand*: ein Klick; Funde sichten. Wieder ausschalten, wenn nur Rauschen kommt.

**N2. `rust-version` (MSRV) im `Cargo.toml`**
- *Was*: `rust-version = "1.xx"` setzen, damit Cargo mit älterem Compiler eine klare Meldung gibt ([Cargo-Doku rust-version][msrv]).
- *Warum*: Turtle wird als Image und Binary ausgeliefert, nicht als Bibliothek; ein MSRV-CI-Job würde kaum etwas finden. Das Feld allein dokumentiert, womit gebaut wird. Kaneo ist kein Rust-Projekt, kein Vergleich möglich.
- *Aufwand*: eine Zeile, kein CI-Job.

**N3. ESLint mit `eslint-plugin-svelte`**
- *Was*: Linter für Svelte-spezifische Muster über svelte-check hinaus ([eslint-plugin-svelte][eps]).
- *Warum nur niedrig*: svelte-check meldet schon Compiler-Warnungen inklusive Barrierefreiheit. Kaneo betreibt einen Linter mit wenigen, gezielt eingeschalteten Regeln und schaltet einige Defaults bewusst ab ([vite.config.ts][k-vite]), das zeigt den Pflegeaufwand. Erst einführen, wenn wiederholt Fehler durchrutschen, die eine Regel gefunden hätte.

### Bewusst nicht übernehmen

| Kaneo-Praxis | Warum nicht für Turtle |
|---|---|
| commitlint, PR-Titel-Check, Conventional Commits ([pr-title.yml][k-prtitle], [commitlint.config.js][k-commitlint]) | Turtle hat eine eigene Konvention (Satz im Imperativ, `Closes #N`/`Refs #N` laut `AGENTS.md`). Conventional Commits brauchen erst, wer daraus automatisch Releases ableitet. |
| semantic-release ([release.config.js][k-release-cfg]) | Turtle schreibt Release-Notizen von Hand in `docs/releases/` und taggt `v*.*.*` für `image.yml`. Automatische Versionierung setzt Conventional Commits voraus (siehe oben). |
| Renovate statt Dependabot ([renovate.json][k-renovate]) | Renovate kann mehr (Dependency Dashboard, Freigabe für Majors), braucht aber eine App und eigene Konfiguration. Dependabot reicht mit `groups` und `cooldown` für Turtles Umfang. |
| TruffleHog selbst installieren, SARIF, Discord ([ci.yml][k-ci], [nightly.yml][k-nightly]) | GitHubs eingebautes Secret Scanning mit Push Protection (H1) ist für öffentliche Repos gratis und braucht keinen Workflow. |
| Socket Security (GitHub-App) | Drittanbieter-App mit eigenem Konto; Dependabot-Alerts und cargo-deny decken das Nötigste ab. |
| CodeRabbit, eigene KI-Review-Bots ([.coderabbit.yaml][k-coderabbit], [code-review.yml][k-codereview]) | Kaneo hat CodeRabbits Auto-Review selbst ausgeschaltet. Turtle reviewt schon mit Agents und `CODING_STANDARDS.md`; ein Bot mehr heißt mehr Kommentare zu lesen. |
| AgentScan, Slop-Labeler, PR-Größen-Labels, Berechtigungsprüfung, mellon, Auto-Assign | Moderation für viele Fremd-PRs ([contribution-policy.json][k-policy] listet ein Kernteam). Bei Turtle gibt es diese Last noch nicht. Alle laufen auf `pull_request_target`, was Kaneo jeweils per zizmor-Ausnahme begründen muss: zusätzliche Angriffsfläche. |
| Coverage-Messung und -Gate | Kaneo hat selbst keins. `CODING_STANDARDS.md` verlangt Tests über die öffentliche Schnittstelle mit erwarteten Werten aus der Spec; eine Prozentzahl misst das nicht. Wer neugierig ist, kann lokal `cargo llvm-cov` ([cargo-llvm-cov][llvmcov]) laufen lassen, ohne CI-Job. |
| Sharding, Blacksmith-Runner, BrowserStack, Helm-Validierung, Nightly-Images | Lösen Probleme von Kaneos Größe (lange Testläufe, Helm-Chart, Mobile-App). Turtles CI ist ein Job. `image.yml` liefert schon `edge` von `main`. |
| Husky | Turtle hat schon `.githooks/pre-commit` mit `core.hooksPath`, ohne npm-Abhängigkeit im Repo-Root. |
| `clippy::pedantic` oder ähnliche Lint-Gruppen | Clippy läuft schon mit `-D warnings`. Die Gruppe `pedantic` beschreibt die Clippy-Doku selbst als streng mit gelegentlichen Fehlalarmen ([Clippy][clippy]); einzelne Lints lieber gezielt in `[lints.clippy]` im `Cargo.toml` einschalten, wenn ein konkreter Fehler das nahelegt. |

Was Kaneo hat und Turtle in eigener Form schon besitzt: Kaneos `i18n:check` und `openapi:check` sind Drift-Prüfungen zwischen Quelle und generiertem Artefakt ([package.json][k-pkg]), wie Turtles Vergleich von `operations.gen.ts` mit dem Operationskatalog in `check.sh`.

## 5. Quellen

Kaneo (Commit `e46680d28a00be1173922ad065ca69243e30ab31`):

- [k-ci]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/ci.yml
- [k-prtitle]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/pr-title.yml
- [k-nightly]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/nightly.yml
- [k-release]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/release.yml
- [k-depgraph]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/dependency-graph.yml
- [k-codereview]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/code-review.yml
- [k-uireview]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/ui-review.yml
- [k-agentscan]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/agentscan.yml
- [k-slop]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/slop-labeler.yml
- [k-prsize]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/pr-size-labeler.yml
- [k-elig]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/contribution-eligibility.yml
- [k-mellon]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/mellon.yml
- [k-assign]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/auto-assign.yml
- [k-actionlint]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/actionlint.yaml
- [k-policy]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/contribution-policy.json
- [k-prtemplate]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/PULL_REQUEST_TEMPLATE.md
- [k-precommit]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.husky/pre-commit
- [k-commitmsg]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.husky/commit-msg
- [k-commitlint]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/commitlint.config.js
- [k-pkg]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/package.json
- [k-vite]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/vite.config.ts
- [k-tsweb]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/apps/web/tsconfig.app.json
- [k-renovate]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/renovate.json
- [k-release-cfg]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/release.config.js
- [k-zizmor]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/zizmor.yml
- [k-coderabbit]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.coderabbit.yaml
- [k-truffle]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.trufflehog-exclude
- Check-Runs von PR #2247: https://github.com/usekaneo/kaneo/pull/2247 (`gh api repos/usekaneo/kaneo/commits/a00aecbc6771506b05e26a767fbc6f83fdc3739c/check-runs`)

Offizielle Tool- und Plattform-Doku:

- [vplus]: https://viteplus.dev/guide/check
- [rustfmt]: https://github.com/rust-lang/rustfmt
- [clippy]: https://doc.rust-lang.org/clippy/
- [deny]: https://embarkstudios.github.io/cargo-deny/
- cargo-deny-action: https://github.com/EmbarkStudios/cargo-deny-action
- [audit]: https://github.com/rustsec/rustsec/tree/main/cargo-audit
- [msrv]: https://doc.rust-lang.org/cargo/reference/rust-version.html
- [llvmcov]: https://github.com/taiki-e/cargo-llvm-cov
- [biome]: https://biomejs.dev/internals/language-support/
- [pps]: https://github.com/sveltejs/prettier-plugin-svelte
- [eps]: https://sveltejs.github.io/eslint-plugin-svelte/
- [actionlint]: https://github.com/rhysd/actionlint
- [zizmor]: https://docs.zizmor.sh/
- [gh-dbopts]: https://docs.github.com/en/code-security/dependabot/working-with-dependabot/dependabot-options-reference
- [gh-secupd]: https://docs.github.com/en/code-security/dependabot/dependabot-security-updates/about-dependabot-security-updates
- [gh-secret]: https://docs.github.com/en/code-security/secret-scanning/introduction/about-secret-scanning
- [gh-push]: https://docs.github.com/en/code-security/secret-scanning/introduction/about-push-protection
- [gh-harden]: https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions
- [codeql]: https://docs.github.com/en/code-security/code-scanning/introduction-to-code-scanning/about-code-scanning-with-codeql
- [codeql-lang]: https://codeql.github.com/docs/codeql-overview/supported-languages-and-frameworks/
- commitlint: https://commitlint.js.org/
- semantic-release: https://github.com/semantic-release/semantic-release
- Renovate: https://docs.renovatebot.com/

[k-ci]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/ci.yml
[k-prtitle]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/pr-title.yml
[k-nightly]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/nightly.yml
[k-release]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/release.yml
[k-depgraph]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/dependency-graph.yml
[k-codereview]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/code-review.yml
[k-uireview]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/ui-review.yml
[k-agentscan]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/agentscan.yml
[k-slop]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/slop-labeler.yml
[k-prsize]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/pr-size-labeler.yml
[k-elig]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/contribution-eligibility.yml
[k-mellon]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/mellon.yml
[k-assign]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/workflows/auto-assign.yml
[k-actionlint]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/actionlint.yaml
[k-policy]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/contribution-policy.json
[k-prtemplate]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.github/PULL_REQUEST_TEMPLATE.md
[k-precommit]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.husky/pre-commit
[k-commitmsg]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.husky/commit-msg
[k-commitlint]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/commitlint.config.js
[k-pkg]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/package.json
[k-vite]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/vite.config.ts
[k-tsweb]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/apps/web/tsconfig.app.json
[k-renovate]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/renovate.json
[k-release-cfg]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/release.config.js
[k-zizmor]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/zizmor.yml
[k-coderabbit]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.coderabbit.yaml
[k-truffle]: https://github.com/usekaneo/kaneo/blob/e46680d28a00be1173922ad065ca69243e30ab31/.trufflehog-exclude
[vplus]: https://viteplus.dev/guide/check
[rustfmt]: https://github.com/rust-lang/rustfmt
[clippy]: https://doc.rust-lang.org/clippy/
[deny]: https://embarkstudios.github.io/cargo-deny/
[audit]: https://github.com/rustsec/rustsec/tree/main/cargo-audit
[msrv]: https://doc.rust-lang.org/cargo/reference/rust-version.html
[llvmcov]: https://github.com/taiki-e/cargo-llvm-cov
[biome]: https://biomejs.dev/internals/language-support/
[pps]: https://github.com/sveltejs/prettier-plugin-svelte
[eps]: https://sveltejs.github.io/eslint-plugin-svelte/
[actionlint]: https://github.com/rhysd/actionlint
[zizmor]: https://docs.zizmor.sh/
[gh-dbopts]: https://docs.github.com/en/code-security/dependabot/working-with-dependabot/dependabot-options-reference
[gh-secupd]: https://docs.github.com/en/code-security/dependabot/dependabot-security-updates/about-dependabot-security-updates
[gh-secret]: https://docs.github.com/en/code-security/secret-scanning/introduction/about-secret-scanning
[gh-push]: https://docs.github.com/en/code-security/secret-scanning/introduction/about-push-protection
[gh-harden]: https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions
[codeql]: https://docs.github.com/en/code-security/code-scanning/introduction-to-code-scanning/about-code-scanning-with-codeql
[codeql-lang]: https://codeql.github.com/docs/codeql-overview/supported-languages-and-frameworks/
