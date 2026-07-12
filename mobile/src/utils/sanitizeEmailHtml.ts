const BLOCKED = 'script,iframe,frame,object,embed,form,input,button,textarea,select,link,meta,base,svg,math,style';

export function sanitizeEmailHtml(html: string): string {
  const document = new DOMParser().parseFromString(html, 'text/html');
  document.querySelectorAll(BLOCKED).forEach((node) => node.remove());
  document.querySelectorAll<HTMLElement>('*').forEach((element) => {
    for (const attribute of [...element.attributes]) {
      const name = attribute.name.toLowerCase();
      const value = attribute.value.trim();
      if (name.startsWith('on') || name === 'srcset' || name === 'action' || name === 'formaction') {
        element.removeAttribute(attribute.name);
      } else if (name === 'style' && /url\s*\(|expression\s*\(|@import/i.test(value)) {
        element.removeAttribute(attribute.name);
      }
    }
  });
  document.querySelectorAll<HTMLImageElement>('img').forEach((image) => {
    if (!image.src.startsWith('data:image/')) {
      const blocked = document.createElement('span');
      blocked.textContent = `[remote image blocked${image.alt ? `: ${image.alt}` : ''}]`;
      image.replaceWith(blocked);
    }
  });
  document.querySelectorAll<HTMLAnchorElement>('a').forEach((anchor) => {
    const href = anchor.getAttribute('href') ?? '';
    if (!/^(https?:|mailto:)/i.test(href)) anchor.removeAttribute('href');
    anchor.target = '_blank';
    anchor.rel = 'noopener noreferrer';
  });
  return `<!doctype html><html><head>
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'">
<meta name="referrer" content="no-referrer">
<style>.blocked-image{display:inline-block;border:1px solid #aaa;padding:4px 6px;color:#555}</style>
</head><body>${document.body.innerHTML}</body></html>`;
}
