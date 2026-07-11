// Generated from email_server::api::PRIVILEGED_ROUTE_INVENTORY.
// Regenerate when the canonical server route inventory changes.
export type ApiMethod = 'GET' | 'POST' | 'PATCH' | 'PUT' | 'DELETE';

const routes: readonly [ApiMethod, RegExp][] = [
  ['GET', /^\/accounts$/], ['POST', /^\/accounts$/],
  ['PATCH', /^\/accounts\/[^/]+$/], ['DELETE', /^\/accounts\/[^/]+$/],
  ['GET', /^\/accounts\/[^/]+\/settings$/], ['GET', /^\/accounts\/[^/]+\/folders$/],
  ['PATCH', /^\/folders\/[^/]+$/], ['POST', /^\/folders\/[^/]+\/mark-read$/],
  ['GET', /^\/folders\/[^/]+\/messages$/], ['POST', /^\/messages\/bulk$/],
  ['GET', /^\/messages\/[^/]+$/], ['PATCH', /^\/messages\/[^/]+$/], ['DELETE', /^\/messages\/[^/]+$/],
  ['GET', /^\/messages\/[^/]+\/attachments$/], ['GET', /^\/messages\/[^/]+\/attachments\/[^/]+$/],
  ['POST', /^\/messages\/[^/]+\/archive$/], ['POST', /^\/messages$/],
  ['GET', /^\/search$/], ['GET', /^\/contacts\/suggest$/],
  ['GET', /^\/app-auth\/me$/], ['POST', /^\/app-auth\/logout$/], ['POST', /^\/app-auth\/refresh$/],
];

export function assertContractRoute(method: string, pathWithQuery: string): void {
  const path = pathWithQuery.split('?', 1)[0];
  if (!routes.some(([candidate, pattern]) => candidate === method && pattern.test(path))) {
    throw new Error(`Mobile request is absent from the generated API contract: ${method} ${path}`);
  }
}
