import { describe, expect, it } from 'vitest';

import { sanitizeEmailHtml } from './sanitizeEmailHtml';

describe('sanitizeEmailHtml', () => {
  it('strips scripts, event handlers, and javascript URLs', () => {
    const sanitized = sanitizeEmailHtml('<script>alert(1)</script><p onmouseover="alert(2)"><a href="javascript:alert(3)">Bad</a></p>');

    expect(sanitized).not.toContain('<script');
    expect(sanitized).not.toContain('onmouseover');
    expect(sanitized).not.toContain('javascript:');
  });

  it('blocks remote images and unsafe style attributes', () => {
    const sanitized = sanitizeEmailHtml('<img src="https://tracker.example/pixel" alt="tracking pixel"><p style="background:url(https://tracker)">Text</p>');

    expect(sanitized).toContain('[remote image blocked: tracking pixel]');
    expect(sanitized).not.toContain('tracker.example');
    expect(sanitized).not.toContain('background:url');
  });

  it('strips srcset while preserving data images', () => {
    const sanitized = sanitizeEmailHtml('<img src="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP" srcset="x 1x">');

    expect(sanitized).not.toContain('srcset');
    expect(sanitized).toContain('<img');
  });
});
