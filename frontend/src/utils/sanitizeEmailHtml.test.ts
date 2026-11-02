import { describe, expect, it } from 'vitest';

import { sanitizeEmailHtml } from './sanitizeEmailHtml';

describe('sanitizeEmailHtml', () => {
  it('removes active content and unsafe attributes', () => {
    const sanitized = sanitizeEmailHtml(
      '<script>alert(1)</script><p onclick="alert(2)" style="background:url(https://tracker)">Hello</p>',
      false,
    );

    expect(sanitized).not.toContain('<script');
    expect(sanitized).not.toContain('onclick');
    expect(sanitized).not.toContain('background:url');
    expect(sanitized).toContain('<p>Hello</p>');
  });

  it('blocks remote images and hardens safe links', () => {
    const sanitized = sanitizeEmailHtml(
      '<img src="https://tracker.example/pixel" alt="tracking pixel"><a href="https://example.com">Open</a>',
      true,
    );

    expect(sanitized).toContain('[remote image blocked: tracking pixel]');
    expect(sanitized).not.toContain('tracker.example');
    expect(sanitized).toContain('target="_blank"');
    expect(sanitized).toContain('rel="noopener noreferrer"');
  });
});
