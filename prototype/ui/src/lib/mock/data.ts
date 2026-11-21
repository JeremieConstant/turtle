/*
 * The dataset of the prototype: the Workspace "Novalutions", its Members, the
 * Docs Agent, seven Areas and their trees. Everything here is made up.
 */
import { contentFor, DEPLOYING_TO_PRODUCTION } from './content'
import type { Area, Author, Entry, Kind } from './types'

const MINUTE = 60_000
const HOUR = 60 * MINUTE
const DAY = 24 * HOUR
const now = Date.now()

export const WORKSPACE = { name: 'Novalutions' }

export const MEMBERS = {
  me: { kind: 'member', id: 'm-mara', name: 'Mara Klein' },
  lena: { kind: 'member', id: 'm-lena', name: 'Lena Hoffmann' },
  tobias: { kind: 'member', id: 'm-tobias', name: 'Tobias Brandt' },
  jonas: { kind: 'member', id: 'm-jonas', name: 'Jonas Weber' },
  aylin: { kind: 'member', id: 'm-aylin', name: 'Aylin Demir' },
  felix: { kind: 'member', id: 'm-felix', name: 'Felix Neumann' },
} satisfies Record<string, Author>

export const DOCS_AGENT: Author = { kind: 'agent', id: 'ag-docs', name: 'Docs Agent' }

export const AREAS: Area[] = [
  { id: 'personal', name: 'Personal', personal: true, role: 'admin' },
  { id: 'platform', name: 'Platform', personal: false, role: 'write' },
  { id: 'product', name: 'Product', personal: false, role: 'write' },
  { id: 'onboarding', name: 'Onboarding', personal: false, role: 'admin' },
  { id: 'sales', name: 'Sales', personal: false, role: 'read' },
  { id: 'operations', name: 'Operations', personal: false, role: 'write' },
  { id: 'archive', name: 'Archive', personal: false, role: 'admin' },
]

/* A small tree language: F is a Folder, P a Page. An optional id pins the
   address of a Page that other content links to. */
type Spec = { kind: Kind; title: string; children: Spec[]; id?: string; html?: string }
const F = (title: string, children: Spec[] = []): Spec => ({ kind: 'folder', title, children })
const P = (title: string, children: Spec[] = [], extra: Partial<Spec> = {}): Spec => ({
  kind: 'page',
  title,
  children,
  ...extra,
})

const TREES: Record<string, Spec[]> = {
  platform: [
    F('Runbooks', [
      F('Deployment', [
        P(
          'Deploying to production',
          [P('Rollback procedure', [], { id: 'p-rollback' }), P('Feature flags during release', [], { id: 'p-flags' })],
          { id: 'p-deploy', html: DEPLOYING_TO_PRODUCTION },
        ),
        P('Deploying to staging'),
        P('Hotfix releases'),
      ]),
      F('Database', [
        P('Postgres failover'),
        P('Restoring from backup', [P('Point-in-time recovery', [P('Verifying a restore')])]),
        P('Running migrations'),
      ]),
      F('Incident response', [
        P('On-call handbook', [P('Escalation contacts'), P('Paging policy')]),
        P('Severity levels'),
        P('Status page updates'),
      ]),
      P('Rotating TLS certificates'),
      P('Scaling the worker pool'),
    ]),
    F('Architecture decisions', [
      P('ADR-001 One monorepo for all services'),
      P('ADR-002 Postgres as the primary store'),
      P('ADR-003 Event bus on NATS'),
      P('ADR-004 Kubernetes on Hetzner'),
      P('ADR-005 OpenTelemetry for tracing'),
      P('ADR-006 Feature flags with Unleash'),
      P('ADR-007 Retire the legacy billing service'),
    ]),
    F('API', [
      P('API overview', [
        P('Authentication', [P('Personal access tokens'), P('OAuth client credentials')]),
        P('Rate limits'),
        P('Errors and status codes'),
        P('Pagination'),
      ]),
      F('Endpoints', [
        P('Accounts'),
        P('Projects', [P('Project members')]),
        P('Invoices'),
        P('Webhooks', [P('Webhook signatures'), P('Retry behaviour')]),
      ]),
      P('Changelog'),
    ]),
    F('Incident reviews', [
      F('2026', [
        P('2026-10-14 Checkout latency spike'),
        P('2026-09-02 Expired certificate on the API gateway'),
        P('2026-07-21 Queue backlog after a deploy'),
        P('2026-05-11 DNS records pointing at the old load balancer'),
      ]),
      F('2025', [P('2025-12-03 Database disk full'), P('2025-08-19 Partial outage in eu-central')]),
      P('Review template'),
    ]),
    F('Infrastructure', [
      P('Cluster overview', [P('Namespaces'), P('Ingress and load balancing')]),
      P('Observability', [P('Dashboards'), P('Alert rules')]),
      P('Secrets management'),
      P('Cost overview'),
    ]),
    P('Platform team'),
  ],
  product: [
    P('Product principles'),
    F('Roadmap', [P('2027 roadmap'), P('Themes for Q1')]),
    F('Specs', [
      P('Quick Notes'),
      P('Search ranking'),
      P('Agent permissions', [P('Open questions')]),
      P('Share Links'),
      P('Import from Confluence'),
    ]),
    F('Research', [P('Interviews with platform teams'), P('Interviews with IT leads'), P('Competitive landscape')]),
    F('Releases', [P('Release 0.4'), P('Release 0.5'), P('Planning release 0.6')]),
    P('Pricing model'),
  ],
  onboarding: [
    P('Welcome to Novalutions'),
    P('Your first week'),
    F('Accounts and access', [P('Setting up your laptop'), P('SSH and VPN'), P('Password manager'), P('GitHub access')]),
    F('Engineering', [
      P('Local development setup'),
      P('Code review guidelines'),
      P('How we write documentation'),
      P('Release process'),
    ]),
    P('Team directory'),
    P('Holidays and time off'),
    P('Expense policy'),
  ],
  sales: [
    P('Sales playbook'),
    F('Pitch', [P('Demo script', [], { id: 'p-demo' }), P('Objection handling'), P('Pitch deck notes')]),
    F('Customers', [P('Hansewerk Logistik'), P('Brandt & Söhne'), P('Stadtwerke Nordhafen')]),
    P('Pricing and discounts'),
    P('Proposal template'),
    F('Partners', [P('Reseller programme'), P('Partner list')]),
  ],
  operations: [
    F('Finance', [P('Invoicing process'), P('Month-end close'), P('Travel expenses')]),
    F('Office', [P('Office access'), P('Visitors'), P('Wi-Fi and printers')]),
    F('Vendors', [P('Hosting providers'), P('Software subscriptions'), P('Ordering hardware')]),
    F('Security', [P('Security policy'), P('Access reviews'), P('Lost or stolen device')]),
    P('Operations calendar'),
    P('Contacts'),
  ],
  archive: [],
}

const QUICK_NOTES: { text: string; ago: number }[] = [
  { text: 'Ask Lena how big the staging database is now\nThe restore test took 40 minutes last time.', ago: 25 * MINUTE },
  { text: 'Retro moves to Thursday 14:00', ago: 3 * HOUR },
  { text: 'Idea: link each alert rule to its runbook\nSo the person on call lands on the right Page straight from the page.', ago: DAY },
  { text: 'Order a USB-C dock for the second desk', ago: 2 * DAY },
  { text: 'Read the Postgres 18 release notes before the upgrade', ago: 4 * DAY },
]

export type Record_ = Entry & {
  html: string
  edited_at: number
  edited_by: Author
  created_at: number
  /** Set on Pages made through create_quick_note; it stays when the note is moved. */
  quick_note?: boolean
}

const people = Object.values(MEMBERS)
let counter = 100
export const nextId = () => `p${++counter}`

export function buildRecords(): Record_[] {
  const records: Record_[] = []
  for (const [areaId, specs] of Object.entries(TREES)) {
    const section = AREAS.find((a) => a.id === areaId)!.name
    const walk = (list: Spec[], parent: string | null, topSection: string, depth: number) => {
      list.forEach((spec, position) => {
        const id = spec.id ?? nextId()
        const n = records.length
        const sectionName = depth === 0 && spec.kind === 'folder' ? spec.title : topSection
        records.push({
          id,
          area_id: areaId,
          parent_id: parent,
          kind: spec.kind,
          title: spec.title,
          position,
          html: spec.kind === 'page' ? (spec.html ?? contentFor(spec.title, sectionName, section)) : '',
          edited_at: now - ((n * 7919) % 40) * HOUR - (n % 50) * MINUTE,
          edited_by: people[n % people.length],
          created_at: now - 90 * DAY,
        })
        walk(spec.children, id, sectionName, depth + 1)
      })
    }
    walk(specs, null, section, 0)
  }

  QUICK_NOTES.forEach((note, position) => {
    const [title, ...rest] = note.text.split('\n')
    records.push({
      id: nextId(),
      area_id: 'personal',
      parent_id: null,
      kind: 'page',
      title,
      position,
      html: rest.length ? `<p>${rest.join('</p><p>')}</p>` : '',
      edited_at: now - note.ago,
      edited_by: MEMBERS.me,
      created_at: now - note.ago,
      quick_note: true,
    })
  })

  // The open Page was last touched by Lena a few minutes ago.
  const deploy = records.find((r) => r.id === 'p-deploy')!
  deploy.edited_at = now - 4 * MINUTE
  deploy.edited_by = MEMBERS.lena
  return records
}
