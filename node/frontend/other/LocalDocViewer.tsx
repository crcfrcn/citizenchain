import { useEffect, useMemo, useRef, useState } from 'react';
import DOMPurify from 'dompurify';
import { marked } from 'marked';
import type { LocalDoc } from '../local-docs.generated';

type TocItem = {
  id: string;
  text: string;
  subtitle: string;
  level: number;
  parentId: string | null;
  children: TocItem[];
};

function escapeHtml(input: string) {
  return input
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function splitPipeRow(line: string) {
  return line
    .trim()
    .replace(/^\|/, '')
    .replace(/\|$/, '')
    .split('|')
    .map((item) => item.trim());
}

function parseMarkdownTable(rawText: string) {
  const lines = rawText
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
  if (lines.length < 2) return null;
  if (!lines[0].includes('|')) return null;
  if (!/^\|?[:\-| ]+\|?$/.test(lines[1])) return null;

  const headers = splitPipeRow(lines[0]);
  const aligns = splitPipeRow(lines[1]).map((cell) => {
    if (cell.startsWith(':') && cell.endsWith(':')) return 'center';
    if (cell.endsWith(':')) return 'right';
    return 'left';
  });

  const thead = `<thead><tr>${headers
    .map((text, idx) => `<th style="text-align:${aligns[idx]}">${escapeHtml(text)}</th>`)
    .join('')}</tr></thead>`;
  const tbody = `<tbody>${lines
    .slice(2)
    .map((line) => {
      const cells = splitPipeRow(line);
      return `<tr>${headers
        .map((_, idx) => {
          const text = cells[idx] ?? '';
          return `<td style="text-align:${aligns[idx]}">${escapeHtml(text)}</td>`;
        })
        .join('')}</tr>`;
    })
    .join('')}</tbody>`;

  return `<table class="rendered-table">${thead}${tbody}</table>`;
}

function upgradeTableCodeBlocks(rootEl: HTMLElement) {
  rootEl.querySelectorAll('pre > code').forEach((block) => {
    const tableHtml = parseMarkdownTable(block.textContent ?? '');
    if (!tableHtml) return;
    const preEl = block.closest('pre');
    if (!preEl) return;
    const wrapper = document.createElement('div');
    wrapper.className = 'table-code-block';
    wrapper.innerHTML = tableHtml;
    if (wrapper.firstElementChild) {
      preEl.replaceWith(wrapper);
    }
  });
}

function stripInlineToc(rootEl: HTMLElement) {
  const normalize = (value: string | null) => (value ?? '').replace(/\s+/g, '');
  const tocHeading = Array.from(rootEl.querySelectorAll('h1, h2, h3')).find(
    (heading) => normalize(getHeadingMainText(heading)) === '目录',
  );
  if (!tocHeading) return;

  let node = tocHeading.nextElementSibling;
  tocHeading.remove();
  while (node) {
    const nextNode = node.nextElementSibling;
    if (/^H[1-6]$/.test(node.tagName)) break;
    node.remove();
    node = nextNode;
  }
}

function stripHorizontalRules(rootEl: HTMLElement) {
  const isRuleText = (text: string | null) => /^[-*]{3,}$/.test((text ?? '').replace(/\s+/g, ''));
  rootEl.querySelectorAll('hr').forEach((el) => el.remove());
  rootEl.querySelectorAll('p').forEach((el) => {
    if (isRuleText(el.textContent)) el.remove();
  });
  rootEl.querySelectorAll('ul,ol').forEach((list) => {
    const items = Array.from(list.children).filter((node) => node.tagName === 'LI');
    if (items.length && items.every((item) => /^[-*]+$/.test((item.textContent ?? '').trim()))) {
      list.remove();
    }
  });
}

function normalizeDocHeading(value: string | null) {
  return (value ?? '')
    .replace(/\s+/g, '')
    .replace(/[《》<>「」『』【】\[\]()]/g, '');
}

function getHeadingMainText(heading: Element) {
  const clone = heading.cloneNode(true) as HTMLElement;
  clone.querySelectorAll('.whitepaper-title-en, .whitepaper-heading-en').forEach((el) => el.remove());
  return (clone.textContent ?? '').trim();
}

function applyDocSpecificClasses(rootEl: HTMLElement) {
  rootEl.querySelectorAll('h1').forEach((heading) => {
    const normalized = normalizeDocHeading(getHeadingMainText(heading));
    if (normalized.includes('白皮书')) {
      heading.classList.add('paper-main-title');
    }
  });
}

function slugify(text: string, usedMap: Map<string, number>) {
  const compact = text
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s-]/gu, '')
    .replace(/\s+/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-|-$/g, '');
  const base = compact || 'section';
  const index = usedMap.get(base) ?? 0;
  usedMap.set(base, index + 1);
  return index === 0 ? base : `${base}-${index}`;
}

function inferTocLevel(text: string, fallbackLevel: number) {
  if (/^\d+\.\d+\.\d+/.test(text)) return 3;
  if (/^\d+\.\d+/.test(text)) return 2;
  if (/^\d+[\.\u3002、)\s]/.test(text)) return 1;
  if (/^第[一二三四五六七八九十百千万0-9]+章/.test(text)) return 1;
  if (/^第[一二三四五六七八九十百千万0-9]+节/.test(text)) return 2;
  if (/^第[一二三四五六七八九十百千万0-9]+条/.test(text)) return 3;
  return fallbackLevel;
}

function buildTocTree(items: TocItem[]) {
  const roots: TocItem[] = [];
  const stack: TocItem[] = [];
  items.forEach((item) => {
    while (stack.length && stack[stack.length - 1].level >= item.level) {
      stack.pop();
    }
    if (stack.length) {
      const parent = stack[stack.length - 1];
      item.parentId = parent.id;
      parent.children.push(item);
    } else {
      roots.push(item);
    }
    stack.push(item);
  });
  return roots;
}

function shouldSkipTocHeading(text: string, doc: LocalDoc) {
  const normalized = normalizeDocHeading(text);
  if (normalized === '目录') return true;
  if (doc.key === 'whitepaper' && normalized.includes('白皮书')) return true;
  return false;
}

function getTocHeadingText(heading: Element, doc: LocalDoc) {
  const subtitleEl =
    doc.key === 'whitepaper'
      ? Array.from(heading.children).find(
          (child) =>
            child.classList.contains('whitepaper-title-en') ||
            child.classList.contains('whitepaper-heading-en'),
        )
      : null;
  const clone = heading.cloneNode(true) as HTMLElement;
  clone.querySelectorAll('.whitepaper-title-en, .whitepaper-heading-en').forEach((el) => el.remove());
  return {
    text: (clone.textContent ?? '').trim(),
    subtitle: (subtitleEl?.textContent ?? '').trim(),
  };
}

function buildTocFromDom(rootEl: HTMLElement, doc: LocalDoc) {
  const used = new Map<string, number>();
  const items: TocItem[] = [];
  rootEl.querySelectorAll('h1, h2, h3').forEach((heading) => {
    const { text, subtitle } = getTocHeadingText(heading, doc);
    if (!text || shouldSkipTocHeading(text, doc)) return;
    const id = slugify(text, used);
    heading.id = id;
    const tagLevel = Number(heading.tagName.slice(1));
    items.push({
      id,
      text,
      subtitle,
      level: inferTocLevel(text, tagLevel),
      parentId: null,
      children: [],
    });
  });
  return { roots: buildTocTree(items), flat: items };
}

function renderMarkdown(markdown: string) {
  const html = marked.parse(markdown, {
    async: false,
    gfm: true,
    breaks: false,
  }) as string;
  return DOMPurify.sanitize(html, {
    USE_PROFILES: { html: true },
  });
}

function flattenToc(items: TocItem[]) {
  const result: TocItem[] = [];
  const visit = (item: TocItem) => {
    result.push(item);
    item.children.forEach(visit);
  };
  items.forEach(visit);
  return result;
}

function containsTocItem(item: TocItem, id: string): boolean {
  if (item.id === id) return true;
  return item.children.some((child) => containsTocItem(child, id));
}

function tocItemIsActive(item: TocItem, activeId: string, expanded: Set<string>) {
  if (item.id === activeId) return true;
  if (!item.children.length || expanded.has(item.id)) return false;
  return containsTocItem(item, activeId);
}

function TocNode({
  item,
  activeId,
  expanded,
  onJump,
  onToggle,
}: {
  item: TocItem;
  activeId: string;
  expanded: Set<string>;
  onJump: (id: string) => void;
  onToggle: (id: string) => void;
}) {
  const hasChildren = item.children.length > 0;
  const isExpanded = expanded.has(item.id);
  const isActive = tocItemIsActive(item, activeId, expanded);
  const nodeClass = `toc-node level-${item.level} ${hasChildren ? 'branch' : 'leaf'}${
    hasChildren && isExpanded ? ' expanded' : ''
  }`;
  const rowClass = `toc-link toc-row ${hasChildren ? 'branch' : 'leaf'} level-${item.level}${
    isActive ? ' active' : ''
  }`;

  return (
    <div className={nodeClass} data-id={item.id}>
      {hasChildren ? (
        <button className={rowClass} data-id={item.id} type="button" onClick={() => onToggle(item.id)}>
          <span className="toc-caret">▸</span>
          <span className="toc-text">
            <span className="toc-text-main">{item.text}</span>
            {item.subtitle ? <span className="toc-text-en">{item.subtitle}</span> : null}
          </span>
        </button>
      ) : (
        <a
          className={rowClass}
          data-id={item.id}
          href={`#${item.id}`}
          onClick={(event) => {
            event.preventDefault();
            onJump(item.id);
          }}
        >
          <span className="toc-caret">•</span>
          <span className="toc-text">
            <span className="toc-text-main">{item.text}</span>
            {item.subtitle ? <span className="toc-text-en">{item.subtitle}</span> : null}
          </span>
        </a>
      )}

      {hasChildren ? (
        <div className="toc-children">
          {item.children.map((child) => (
            <TocNode
              key={child.id}
              item={child}
              activeId={activeId}
              expanded={expanded}
              onJump={onJump}
              onToggle={onToggle}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}

type Props = {
  doc: LocalDoc;
};

export function LocalDocViewer({ doc }: Props) {
  const shellRef = useRef<HTMLElement | null>(null);
  const articleRef = useRef<HTMLElement | null>(null);
  const [tocItems, setTocItems] = useState<TocItem[]>([]);
  const [activeId, setActiveId] = useState('');
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const [showToTop, setShowToTop] = useState(false);
  const html = useMemo(() => renderMarkdown(doc.markdown), [doc.markdown]);
  // 正文目录移除和标题类由副作用直接写入 DOM；保持属性对象引用稳定，避免状态更新时 React 重写原始 HTML。
  const htmlPayload = useMemo(() => ({ __html: html }), [html]);
  const flatToc = useMemo(() => flattenToc(tocItems), [tocItems]);

  useEffect(() => {
    const article = articleRef.current;
    const shell = shellRef.current;
    if (!article || !shell) return undefined;

    upgradeTableCodeBlocks(article);
    stripInlineToc(article);
    stripHorizontalRules(article);
    applyDocSpecificClasses(article);

    const { roots, flat } = buildTocFromDom(article, doc);
    setTocItems(roots);
    setExpanded(new Set());
    setActiveId(flat[0]?.id ?? '');

    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            setActiveId((entry.target as HTMLElement).id);
          }
        });
      },
      { root: shell, rootMargin: '-40% 0px -45% 0px', threshold: 0.1 },
    );
    flat.forEach((item) => {
      const heading = document.getElementById(item.id);
      if (heading) observer.observe(heading);
    });

    const handleScroll = () => {
      setShowToTop(shell.scrollTop > 260);
    };
    shell.addEventListener('scroll', handleScroll);
    handleScroll();

    return () => {
      observer.disconnect();
      shell.removeEventListener('scroll', handleScroll);
    };
  }, [doc, html]);

  const displayTitle = '公民区块链白皮书';
  const eyebrow = 'CitizenChain Whitepaper';

  const jumpToHeading = (id: string) => {
    const heading = document.getElementById(id);
    if (!heading) return;
    heading.scrollIntoView({ behavior: 'smooth', block: 'start' });
    setActiveId(id);
  };

  const toggleTocNode = (id: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  return (
    <section ref={shellRef} className={`local-doc-shell doc-${doc.key}`}>
      <header className="hero">
        <p className="eyebrow">{eyebrow}</p>
        <h1>{displayTitle}</h1>
      </header>

      <div className="layout">
        <aside className="toc" aria-label={`${displayTitle}目录`}>
          <h2>
            <span className="toc-title-main">目录</span>
            <span className="toc-title-en">Table of Contents</span>
          </h2>
          <nav id="toc-nav">
            {tocItems.length ? (
              tocItems.map((item) => (
                <TocNode
                  key={item.id}
                  item={item}
                  activeId={activeId}
                  expanded={expanded}
                  onJump={jumpToHeading}
                  onToggle={toggleTocNode}
                />
              ))
            ) : (
              <p className="toc-empty">暂无可用目录</p>
            )}
          </nav>
        </aside>

        <main className="paper">
          <article
            ref={articleRef}
            id="content"
            className="markdown-body"
            dangerouslySetInnerHTML={htmlPayload}
          />
        </main>
      </div>

      <button
        className={`to-top${showToTop ? ' visible' : ''}`}
        type="button"
        aria-label="回到顶部"
        onClick={() => {
          shellRef.current?.scrollTo({ top: 0, behavior: 'smooth' });
        }}
      >
        ↑
      </button>

      <span className="doc-anchor-sentinel" data-count={flatToc.length} />
    </section>
  );
}
