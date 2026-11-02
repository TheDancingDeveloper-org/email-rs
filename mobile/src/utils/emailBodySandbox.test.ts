import { describe, expect, it } from 'vitest';
import { EMAIL_BODY_SANDBOX } from './emailBodySandbox';

describe('email body sandbox', () => {
  it('allows links to open outside the sandbox without enabling active content', () => {
    expect(EMAIL_BODY_SANDBOX.split(' ')).toEqual([
      'allow-popups',
      'allow-popups-to-escape-sandbox',
    ]);
    expect(EMAIL_BODY_SANDBOX).not.toContain('allow-scripts');
    expect(EMAIL_BODY_SANDBOX).not.toContain('allow-same-origin');
  });
});
