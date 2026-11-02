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

  it('strips javascript URLs, srcset, and unsafe style attributes', () => {
    const sanitized = sanitizeEmailHtml(
      '<a href="javascript:alert(1)">Bad</a><img src="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP" srcset="x 1x"><p style="color:red;background-image:url(https://tracker)">Text</p>',
      false,
    );

    expect(sanitized).not.toContain('javascript:');
    expect(sanitized).not.toContain('srcset');
    expect(sanitized).not.toContain('background-image');
    expect(sanitized).toContain('<p>Text</p>');
  });
});
