import { Search, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { PaperHeading, SearchResult } from "../types";

interface SearchDialogProps {
  open: boolean;
  article: HTMLElement | null;
  headings: PaperHeading[];
  onClose: () => void;
  onNavigate: (id: string) => void;
}

function buildResults(
  article: HTMLElement | null,
  headings: PaperHeading[],
  query: string,
): SearchResult[] {
  const normalized = query.trim().toLocaleLowerCase();
  if (!article || normalized.length < 2) {
    return [];
  }

  const headingById = new Map(headings.map((heading) => [heading.id, heading]));
  const results = new Map<string, SearchResult>();
  const candidates = article.querySelectorAll<HTMLElement>(
    "h1[id], h2[id], h3[id], h4[id], p, li, figcaption",
  );
  let currentHeading = headings[0]?.id ?? "introduction";

  for (const candidate of candidates) {
    if (candidate.matches("h1[id], h2[id], h3[id], h4[id]")) {
      currentHeading = candidate.id;
    }
    const text = candidate.textContent?.replace(/\s+/g, " ").trim() ?? "";
    const matchAt = text.toLocaleLowerCase().indexOf(normalized);
    if (matchAt < 0) {
      continue;
    }
    const targetId = candidate.id || currentHeading;
    if (results.has(targetId)) {
      continue;
    }
    const start = Math.max(0, matchAt - 72);
    const end = Math.min(text.length, matchAt + normalized.length + 116);
    const excerpt = `${start > 0 ? "…" : ""}${text.slice(start, end)}${
      end < text.length ? "…" : ""
    }`;
    results.set(targetId, {
      id: targetId,
      section: headingById.get(targetId)?.title ?? "Paper text",
      excerpt,
    });
    if (results.size >= 12) {
      break;
    }
  }
  return [...results.values()];
}

export function SearchDialog({
  open,
  article,
  headings,
  onClose,
  onNavigate,
}: SearchDialogProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const results = useMemo(
    () => buildResults(article, headings, query),
    [article, headings, query],
  );

  useEffect(() => {
    if (!open) {
      setQuery("");
      return;
    }
    const frame = window.requestAnimationFrame(() => inputRef.current?.focus());
    return () => window.cancelAnimationFrame(frame);
  }, [open]);

  if (!open) {
    return null;
  }

  return (
    <div className="modal-layer" role="presentation" onMouseDown={onClose}>
      <section
        className="search-dialog"
        role="dialog"
        aria-modal="true"
        aria-label="Search paper"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="search-dialog-input">
          <Search size={18} aria-hidden="true" />
          <input
            ref={inputRef}
            value={query}
            placeholder="Search the paper"
            aria-label="Search the paper"
            onChange={(event) => setQuery(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                onClose();
              } else if (event.key === "Enter" && results[0]) {
                onNavigate(results[0].id);
                onClose();
              }
            }}
          />
          <button
            type="button"
            className="icon-button"
            aria-label="Close search"
            title="Close"
            onClick={onClose}
          >
            <X size={17} />
          </button>
        </div>
        <div className="search-results" aria-live="polite">
          {!query.trim() && (
            <p className="search-empty">Search headings, text, figures, and references.</p>
          )}
          {query.trim().length === 1 && (
            <p className="search-empty">Enter at least two characters.</p>
          )}
          {query.trim().length >= 2 && results.length === 0 && (
            <p className="search-empty">No matches in this paper.</p>
          )}
          {results.map((result) => (
            <button
              type="button"
              className="search-result"
              key={`${result.id}:${result.excerpt}`}
              onClick={() => {
                onNavigate(result.id);
                onClose();
              }}
            >
              <span>{result.section}</span>
              <small>{result.excerpt}</small>
            </button>
          ))}
        </div>
      </section>
    </div>
  );
}
