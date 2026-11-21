/*
 * Page content for the dataset. The open Page "Deploying to production" is
 * written out in full; the rest is short, plausible content built from the
 * kind of Page it is, so search has something real to find.
 */

export const DEPLOYING_TO_PRODUCTION = `
<p>This runbook describes how a change gets from a merged pull request to production. It applies to every service in the monorepo that runs on the main cluster. If you are shipping a fix for an ongoing incident, read the hotfix section first and keep the incident channel informed while you work.</p>
<div data-callout="note"><p>Production deploys run from Monday to Thursday between 09:00 and 16:00. Outside that window, deploy only for an incident and with a second person on the call.</p></div>
<h2>Before you deploy</h2>
<p>Every deploy starts from a green build on <code>main</code>. The pipeline builds one image per service, tags it with the commit hash and pushes it to the registry. Nothing is built on the way to production, so the image that passed staging is exactly the image that goes live.</p>
<ul data-type="taskList">
<li data-type="taskItem" data-checked="true"><p>The change has been on staging for at least one hour without new alerts</p></li>
<li data-type="taskItem" data-checked="true"><p>Database migrations are backwards compatible with the running version</p></li>
<li data-type="taskItem" data-checked="false"><p>Feature flags for unfinished work are switched off in production</p></li>
<li data-type="taskItem" data-checked="false"><p>The release note is written in the changelog Page</p></li>
<li data-type="taskItem" data-checked="false"><p>Someone from the owning team is available for the next 30 minutes</p></li>
</ul>
<p>If a migration is not backwards compatible, split it. Ship the additive part first, deploy the code that uses it, and remove the old columns in a later release. The <a href="/p/p-flags/feature-flags-during-release">feature flags during release</a> Page explains how to hide code paths that depend on a migration that has not run yet.</p>
<h2>Running the deploy</h2>
<p>Deploys are started from the command line with the platform CLI. It talks to the deploy controller, which rolls the new image out one availability zone at a time and watches the error rate and latency of each zone before it moves on.</p>
<pre><code class="language-bash">plat deploy checkout-api \\
  --image registry.novalutions.internal/checkout-api:4f2a9c1 \\
  --env production \\
  --strategy zonal \\
  --watch</code></pre>
<p>The <code>--watch</code> flag keeps the command attached until the rollout has finished or stopped. Leave it running in a terminal you can see. A rollout that stalls for more than ten minutes in one zone is stopped by the controller and reported in the deploy channel.</p>
<h3>What the controller checks</h3>
<p>Between zones the controller compares the new pods with the old ones over a five-minute window. It stops the rollout as soon as one of the thresholds below is crossed and leaves the remaining zones on the previous image.</p>
<table>
<tr><th><p>Signal</p></th><th><p>Threshold</p></th><th><p>Window</p></th></tr>
<tr><td><p>HTTP 5xx rate</p></td><td><p>Above 0.5 %</p></td><td><p>5 minutes</p></td></tr>
<tr><td><p>p95 latency</p></td><td><p>20 % above baseline</p></td><td><p>5 minutes</p></td></tr>
<tr><td><p>Pod restarts</p></td><td><p>More than 2 per pod</p></td><td><p>10 minutes</p></td></tr>
<tr><td><p>Queue lag</p></td><td><p>Above 30 seconds</p></td><td><p>5 minutes</p></td></tr>
</table>
<p>The baseline is taken from the same hour on the previous three working days, so a deploy during a quiet afternoon is not compared with the morning peak.</p>
<h2>After the deploy</h2>
<p>When the last zone is done, the controller posts a summary with the image, the duration and the person who started it. Check the service dashboard for another fifteen minutes. Error budgets are tracked per service, and a deploy that burns more than a tenth of the monthly budget gets a short note in the incident reviews Area, even if nobody was paged.</p>
<blockquote><p>A deploy is finished when the people on call can stop thinking about it, not when the command returns.</p></blockquote>
<h2>When something goes wrong</h2>
<p>If the controller stops a rollout, the zones that already run the new image stay on it until you decide. Most of the time the right answer is to roll back and look at the cause in peace. The <a href="/p/p-rollback/rollback-procedure">rollback procedure</a> explains the command, how long it takes and which migrations cannot be undone.</p>
<p>Do not deploy a second fix on top of a stopped rollout. Roll back first, then ship the fix as a new deploy, so the controller has a clean baseline to compare against. If the problem affects customers, open an incident before you do anything else; the on-call engineer coordinates from there.</p>
<h2>Hotfixes</h2>
<p>A hotfix follows the same path with two differences: it may run outside the deploy window, and staging time can be shortened to fifteen minutes when the incident lead agrees. Everything else stays the same. Write down in the incident channel who approved the shortcut, so the review can find it later.</p>
`

const para = (...sentences: string[]) => `<p>${sentences.join(' ')}</p>`

/** Short content for a Page, chosen by what kind of Page its title suggests. */
export function contentFor(title: string, section: string, area: string): string {
  if (title.startsWith('ADR-')) {
    const subject = title.replace(/^ADR-\d+\s*/, '')
    return [
      para('<strong>Status:</strong> accepted.'),
      '<h2>Context</h2>',
      para(`We had to settle how the platform handles "${subject.toLowerCase()}".`, 'Several teams had started with their own approach, which made on-call work harder than it needed to be.'),
      '<h2>Decision</h2>',
      para(`${subject}.`, 'New services follow this from the start; existing services move over when they are next touched.'),
      '<h2>Consequences</h2>',
      para('One way of doing it means one runbook and one set of dashboards.', 'The cost is a migration for the two services that did it differently.'),
    ].join('')
  }
  if (/^\d{4}-\d{2}-\d{2}/.test(title)) {
    const subject = title.replace(/^\d{4}-\d{2}-\d{2}\s*/, '')
    return [
      '<h2>Summary</h2>',
      para(`${subject}. Customers saw slower responses or errors for about 40 minutes until the cause was found and fixed.`),
      '<h2>Timeline</h2>',
      '<ul><li><p>14:02 first alert in the on-call channel</p></li><li><p>14:09 incident opened, severity 2</p></li><li><p>14:31 cause found, fix prepared</p></li><li><p>14:44 metrics back to baseline</p></li></ul>',
      '<h2>Follow-ups</h2>',
      '<ul data-type="taskList"><li data-type="taskItem" data-checked="true"><p>Add an alert for the missing signal</p></li><li data-type="taskItem" data-checked="false"><p>Update the runbook with what we learned</p></li></ul>',
    ].join('')
  }
  if (section === 'API') {
    return [
      para(`This Page documents ${title.toLowerCase()} in the public API.`, 'All requests go to <code>https://api.novalutions.example/v2</code> and carry a bearer token.'),
      '<pre><code class="language-bash">curl -H "Authorization: Bearer $TOKEN" \\\n  https://api.novalutions.example/v2/accounts</code></pre>',
      para('Responses are JSON. Errors follow the format described on the errors Page.'),
    ].join('')
  }
  if (section === 'Runbooks' || section === 'Infrastructure') {
    return [
      para(`Use this runbook for ${title.toLowerCase()}.`, 'Read it once before you start, then follow the steps in order.'),
      '<ol><li><p>Announce the work in the platform channel.</p></li><li><p>Check the service dashboard for ongoing alerts.</p></li><li><p>Run the steps below and note anything unexpected.</p></li></ol>',
      '<div data-callout="note"><p>If a step fails, stop and ask in the on-call channel before you improvise.</p></div>',
    ].join('')
  }
  return [
    para(`What the ${area} team knows about ${title.toLowerCase()}, in one place.`, 'Keep it short and current, and link to other Pages instead of copying them.'),
    '<h2>Owner</h2>',
    para(`The ${area} team. Questions go to the ${area.toLowerCase()} channel.`),
  ].join('')
}
