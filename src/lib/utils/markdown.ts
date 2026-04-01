/**
 * Minimal markdown-to-HTML renderer.
 *
 * Supports: headings (#, ##, ###), bold (**text**), italic (*text*),
 * bold+italic (***text***), inline code (`code`), unordered lists (- / *),
 * ordered lists (1.), horizontal rules (---), links ([label](url)), paragraphs.
 *
 * All input is HTML-escaped before pattern processing to prevent XSS.
 * Safe to use with {@html} in Svelte templates.
 */
export function renderMarkdown(md: string): string {
	if (!md) return '';

	const lines = md.split('\n').map(escapeHtml);
	const out: string[] = [];
	let inUl = false;
	let inOl = false;

	const closeList = () => {
		if (inUl) {
			out.push('</ul>');
			inUl = false;
		}
		if (inOl) {
			out.push('</ol>');
			inOl = false;
		}
	};

	for (const raw of lines) {
		const line = applyInline(raw);

		if (/^### /.test(line)) {
			closeList();
			out.push(`<h3>${line.slice(4)}</h3>`);
		} else if (/^## /.test(line)) {
			closeList();
			out.push(`<h2>${line.slice(3)}</h2>`);
		} else if (/^# /.test(line)) {
			closeList();
			out.push(`<h1>${line.slice(2)}</h1>`);
		} else if (/^---+$/.test(line)) {
			closeList();
			out.push('<hr>');
		} else if (/^[-*] /.test(line)) {
			if (inOl) {
				out.push('</ol>');
				inOl = false;
			}
			if (!inUl) {
				out.push('<ul>');
				inUl = true;
			}
			out.push(`<li>${line.slice(2)}</li>`);
		} else if (/^\d+\. /.test(line)) {
			if (inUl) {
				out.push('</ul>');
				inUl = false;
			}
			if (!inOl) {
				out.push('<ol>');
				inOl = true;
			}
			out.push(`<li>${line.replace(/^\d+\. /, '')}</li>`);
		} else if (line.trim() === '') {
			closeList();
			// blank line — paragraph separator, no output
		} else {
			closeList();
			out.push(`<p>${line}</p>`);
		}
	}

	closeList();
	return out.join('');
}

function escapeHtml(str: string): string {
	return str
		.replace(/&/g, '&amp;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;')
		.replace(/"/g, '&quot;')
		.replace(/'/g, '&#39;');
}

function applyInline(line: string): string {
	// First, extract inline code spans so that emphasis and links
	// are not processed inside backticks.
	const codeSpans: string[] = [];
	line = line.replace(/`([^`]+)`/g, (_match, code) => {
		const index = codeSpans.length;
		codeSpans.push(code);
		return `__CODE_SPAN_${index}__`;
	});

	// Bold + italic (***text***) — must come before bold/italic individually
	line = line.replace(/\*\*\*([^*]+)\*\*\*/g, '<strong><em>$1</em></strong>');
	// Bold (**text**)
	line = line.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
	// Italic (*text*)
	line = line.replace(/\*([^*]+)\*/g, '<em>$1</em>');
	// Links ([label](url)) — only allow safe URL schemes
	line = line.replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_match, label, href) => {
		const safe = isSafeUrl(href);
		return safe ? `<a href="${href}" rel="noopener noreferrer">${label}</a>` : label;
	});

	// Restore inline code spans.
	line = line.replace(/__CODE_SPAN_(\d+)__/g, (_match, indexStr) => {
		const index = Number(indexStr);
		const code = codeSpans[index] ?? '';
		return `<code>${code}</code>`;
	});
	return line;
}

/** Allow only http, https, root-relative, document-relative, and hash links. */
export function isSafeUrl(href: string): boolean {
	if (href.startsWith('#')) return true;
	// Relative paths: ./foo, ../foo, foo/bar — but NOT protocol-relative //
	if (href.startsWith('./') || href.startsWith('../')) return true;
	if (href.startsWith('/')) {
		// Allow single-slash root-relative paths ("/", "/foo"), but reject
		// protocol-relative URLs like "//evil.example".
		return href.length === 1 || href[1] !== '/';
	}
	try {
		const url = new URL(href);
		return url.protocol === 'http:' || url.protocol === 'https:';
	} catch {
		return false;
	}
}
