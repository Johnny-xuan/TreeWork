import {
  BookOpen,
  Check,
  ChevronDown,
  Download,
  Menu,
  Moon,
  Quote,
  Search,
  Settings2,
  Sun,
  X,
  ZoomIn,
} from "lucide-react";
import { MarkGithubIcon } from "@primer/octicons-react";
import renderMathInElement from "katex/contrib/auto-render";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import overviewImage from "../../paper/assets/persistent_project_state_infographic_8k_ultra_clear.png";
import comparisonImage from "../../paper/assets/two_panel_agent_workflow_comparison_4k_final.png";
import treeworkIcon from "../../plugins/treework/assets/treework-icon.svg";
import paperSource from "./generated/paper.html?raw";
import { ReadingSettings } from "./components/ReadingSettings";
import { SearchDialog } from "./components/SearchDialog";
import type {
  PaperHeading,
  PaperSection,
  ReaderTheme,
  ReadingMeasure,
} from "./types";

const PAPER_VERSION = "v0.1.4";
const PDF_URL =
  "https://github.com/Johnny-xuan/TreeWork/releases/download/v0.1.4/TreeWork-paper-draft-v0.1.4.pdf";
const REPOSITORY_URL = "https://github.com/Johnny-xuan/TreeWork";
const CITATION_TEXT =
  "Zhongxuan Song. TreeWork: State-Native Project Memory for Long-Horizon Coding Agents. 2026.";
const CITATION_BIBTEX = `@article{song2026treework,
  title  = {TreeWork: State-Native Project Memory for Long-Horizon Coding Agents},
  author = {Song, Zhongxuan},
  year   = {2026},
  url    = {https://github.com/Johnny-xuan/TreeWork}
}`;

function storedValue<T extends string>(key: string, fallback: T): T {
  const value = window.localStorage.getItem(key);
  return (value as T | null) ?? fallback;
}

function cleanHeading(heading: HTMLElement): PaperHeading {
  const clone = heading.cloneNode(true) as HTMLElement;
  const numberNode = clone.querySelector(
    ".header-section-number, .section-number",
  );
  const number = numberNode?.textContent?.trim() ?? "";
  numberNode?.remove();
  clone.querySelector(".heading-anchor")?.remove();
  return {
    id: heading.id,
    level: Number(heading.tagName.slice(1)),
    number,
    title: clone.textContent?.replace(/\s+/g, " ").trim() ?? heading.id,
  };
}

function groupSections(headings: PaperHeading[]): PaperSection[] {
  const sections: PaperSection[] = [];
  for (const heading of headings) {
    if (heading.level === 1) {
      sections.push({ ...heading, children: [] });
    } else {
      sections.at(-1)?.children.push(heading);
    }
  }
  return sections;
}

function useSystemTheme(theme: ReaderTheme) {
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      document.documentElement.dataset.theme =
        theme === "system" ? (media.matches ? "dark" : "light") : theme;
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);
}

function SectionLabel({ heading }: { heading: PaperHeading }) {
  return (
    <>
      {heading.number && <span className="nav-number">{heading.number}</span>}
      <span>{heading.title}</span>
    </>
  );
}

export function App() {
  const articleRef = useRef<HTMLElement>(null);
  const [headings, setHeadings] = useState<PaperHeading[]>([]);
  const [activeHeading, setActiveHeading] = useState("abstract");
  const [leftOpen, setLeftOpen] = useState(
    () => window.innerWidth >= 760,
  );
  const [searchOpen, setSearchOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [citationOpen, setCitationOpen] = useState(false);
  const [copied, setCopied] = useState("");
  const [progress, setProgress] = useState(0);
  const [lightbox, setLightbox] = useState<{ src: string; alt: string } | null>(
    null,
  );
  const [theme, setTheme] = useState<ReaderTheme>(() =>
    storedValue("treework-paper-theme", "system"),
  );
  const [measure, setMeasure] = useState<ReadingMeasure>(() =>
    storedValue("treework-paper-measure", "standard"),
  );
  const [textScale, setTextScale] = useState(() =>
    Number(window.localStorage.getItem("treework-paper-text-scale") ?? "0"),
  );
  useSystemTheme(theme);

  const paperHtml = useMemo(
    () =>
      paperSource
        .replace(
          "__TREEWORK_PAPER_ASSET__/persistent_project_state_infographic_8k_ultra_clear.png",
          overviewImage,
        )
        .replace(
          "__TREEWORK_PAPER_ASSET__/two_panel_agent_workflow_comparison_4k_final.png",
          comparisonImage,
        ),
    [],
  );
  const paperMarkup = useMemo(() => ({ __html: paperHtml }), [paperHtml]);
  const sections = useMemo(() => groupSections(headings), [headings]);
  const activeSection = useMemo(() => {
    let current = sections[0];
    for (const section of sections) {
      const sectionIndex = headings.findIndex((item) => item.id === section.id);
      const activeIndex = headings.findIndex((item) => item.id === activeHeading);
      if (sectionIndex <= activeIndex) {
        current = section;
      }
    }
    return current;
  }, [activeHeading, headings, sections]);

  const navigateTo = useCallback((id: string) => {
    const target = document.getElementById(id);
    if (!target) {
      return;
    }
    window.history.pushState(null, "", `#${id}`);
    target.scrollIntoView({ behavior: "smooth", block: "start" });
    setActiveHeading(id);
    if (window.innerWidth < 760) {
      setLeftOpen(false);
    }
  }, []);

  useEffect(() => {
    const article = articleRef.current;
    if (!article) {
      return;
    }
    const discovered = Array.from(
      article.querySelectorAll<HTMLElement>("h1[id], h2[id], h3[id], h4[id]"),
    ).map(cleanHeading);
    setHeadings(discovered);
    renderMathInElement(article, {
      delimiters: [
        { left: "\\[", right: "\\]", display: true },
        { left: "\\(", right: "\\)", display: false },
      ],
      ignoredClasses: ["equation-number"],
      throwOnError: false,
      strict: "ignore",
    });

    const initialId = window.location.hash.slice(1);
    if (initialId && document.getElementById(initialId)) {
      setActiveHeading(initialId);
      window.requestAnimationFrame(() =>
        document.getElementById(initialId)?.scrollIntoView(),
      );
    } else {
      const saved = Number(window.localStorage.getItem("treework-paper-scroll") ?? 0);
      if (saved > 0) {
        window.requestAnimationFrame(() => window.scrollTo({ top: saved }));
      }
    }

    const observed = discovered
      .map((heading) => document.getElementById(heading.id))
      .filter((element): element is HTMLElement => Boolean(element));
    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries
          .filter((entry) => entry.isIntersecting)
          .sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top);
        if (visible[0]?.target instanceof HTMLElement) {
          setActiveHeading(visible[0].target.id);
        }
      },
      { rootMargin: "-72px 0px -72% 0px", threshold: [0, 1] },
    );
    observed.forEach((element) => observer.observe(element));
    return () => {
      observer.disconnect();
    };
  }, [paperHtml]);

  useEffect(() => {
    let frame = 0;
    const onScroll = () => {
      window.cancelAnimationFrame(frame);
      frame = window.requestAnimationFrame(() => {
        const scrollable = document.documentElement.scrollHeight - window.innerHeight;
        setProgress(scrollable > 0 ? Math.min(1, window.scrollY / scrollable) : 0);
        window.localStorage.setItem("treework-paper-scroll", String(window.scrollY));
      });
    };
    window.addEventListener("scroll", onScroll, { passive: true });
    onScroll();
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("scroll", onScroll);
    };
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target;
      const inControl =
        target instanceof Element &&
        Boolean(target.closest("input, textarea, select, button, [contenteditable='true']"));
      if ((event.key === "/" && !inControl) || ((event.metaKey || event.ctrlKey) && event.key === "k")) {
        event.preventDefault();
        setSearchOpen(true);
      } else if (event.key === "Escape") {
        setSearchOpen(false);
        setSettingsOpen(false);
        setCitationOpen(false);
        setLightbox(null);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => {
    const article = articleRef.current;
    if (!article) {
      return;
    }
    const onClick = (event: MouseEvent) => {
      const target = event.target;
      if (!(target instanceof Element)) {
        return;
      }
      const anchor = target.closest<HTMLAnchorElement>("a[href^='#']");
      if (anchor) {
        const id = anchor.getAttribute("href")?.slice(1);
        if (id) {
          event.preventDefault();
          navigateTo(id);
        }
        return;
      }
      const image = target.closest<HTMLImageElement>("figure img");
      if (image) {
        setLightbox({ src: image.src, alt: image.alt });
      }
    };
    article.addEventListener("click", onClick);
    return () => article.removeEventListener("click", onClick);
  }, [navigateTo]);

  useEffect(() => {
    window.localStorage.setItem("treework-paper-theme", theme);
    window.localStorage.setItem("treework-paper-measure", measure);
    window.localStorage.setItem("treework-paper-text-scale", String(textScale));
  }, [measure, textScale, theme]);

  const copyCitation = async (format: "text" | "bibtex") => {
    await navigator.clipboard.writeText(
      format === "text" ? CITATION_TEXT : CITATION_BIBTEX,
    );
    setCopied(format);
    window.setTimeout(() => setCopied(""), 1600);
  };

  const toggleTheme = () => {
    setTheme((current) => {
      const resolved = document.documentElement.dataset.theme;
      if (current === "system") {
        return resolved === "dark" ? "light" : "dark";
      }
      return current === "dark" ? "light" : "dark";
    });
  };

  return (
    <div
      className={`reader-shell ${leftOpen ? "has-left-rail" : ""}`}
      data-measure={measure}
      data-text-scale={textScale}
    >
      <header className="reader-topbar">
        <div className="topbar-primary">
          <button
            type="button"
            className="icon-button"
            aria-label={leftOpen ? "Hide table of contents" : "Show table of contents"}
            title={leftOpen ? "Hide contents" : "Show contents"}
            onClick={() => setLeftOpen((value) => !value)}
          >
            {leftOpen ? <X size={17} /> : <Menu size={18} />}
          </button>
          <a className="paper-brand" href="#top" onClick={(event) => {
            event.preventDefault();
            window.scrollTo({ top: 0, behavior: "smooth" });
          }}>
            <img src={treeworkIcon} alt="" />
            <span>TreeWork</span>
            <small>Web Paper</small>
          </a>
          <span className="current-location">
            {activeSection?.number && `${activeSection.number} `}
            {activeSection?.title ?? "Paper"}
          </span>
        </div>

        <div className="topbar-actions">
          <button
            type="button"
            className="search-button"
            aria-label="Search paper"
            onClick={() => setSearchOpen(true)}
          >
            <Search size={16} />
            <span>Search</span>
            <kbd>/</kbd>
          </button>
          <a className="compact-action" href={PDF_URL} title="Download PDF">
            <Download size={16} />
            <span>PDF</span>
          </a>
          <a
            className="icon-button"
            href={REPOSITORY_URL}
            aria-label="Open GitHub repository"
            title="GitHub repository"
          >
            <MarkGithubIcon size={17} />
          </a>
          <button
            type="button"
            className="icon-button"
            aria-label="Cite this paper"
            title="Citation"
            onClick={() => setCitationOpen(true)}
          >
            <Quote size={17} />
          </button>
          <button
            type="button"
            className="icon-button"
            aria-label="Toggle color theme"
            title="Toggle theme"
            onClick={toggleTheme}
          >
            {document.documentElement.dataset.theme === "dark" ? (
              <Sun size={17} />
            ) : (
              <Moon size={17} />
            )}
          </button>
          <button
            type="button"
            className="icon-button"
            aria-label="Open reading settings"
            title="Reading settings"
            onClick={() => setSettingsOpen((value) => !value)}
          >
            <Settings2 size={17} />
          </button>
        </div>
      </header>

      {leftOpen && <button className="mobile-scrim" aria-label="Close contents" onClick={() => setLeftOpen(false)} />}
      <aside
        className="chapter-rail"
        aria-label="Paper contents"
        aria-hidden={!leftOpen}
        inert={!leftOpen}
      >
        <div className="rail-heading">
          <BookOpen size={15} />
          <span>Contents</span>
        </div>
        <nav>
          {sections.map((section) => {
            const active = activeSection?.id === section.id;
            return (
              <div className={`chapter-item ${active ? "is-active" : ""}`} key={section.id}>
                <a href={`#${section.id}`} onClick={(event) => {
                  event.preventDefault();
                  navigateTo(section.id);
                }}>
                  <SectionLabel heading={section} />
                  {section.children.length > 0 && <ChevronDown size={13} aria-hidden="true" />}
                </a>
                {active && section.children.length > 0 && (
                  <div className="chapter-children">
                    {section.children
                      .filter((heading) => heading.level <= 2)
                      .map((heading) => (
                        <a
                          key={heading.id}
                          className={activeHeading === heading.id ? "is-current" : ""}
                          href={`#${heading.id}`}
                          onClick={(event) => {
                            event.preventDefault();
                            navigateTo(heading.id);
                          }}
                        >
                          <SectionLabel heading={heading} />
                        </a>
                      ))}
                  </div>
                )}
              </div>
            );
          })}
        </nav>
        <div className="rail-progress" aria-label="Reading progress">
          <div className="rail-progress-label">
            <span>Reading progress</span>
            <output>{Math.round(progress * 100)}%</output>
          </div>
          <div className="rail-progress-track">
            <i style={{ width: `${Math.round(progress * 100)}%` }} />
          </div>
        </div>
        <footer>
          <span>{PAPER_VERSION}</span>
          <span>System paper</span>
        </footer>
      </aside>

      <main className="reader-main" id="top">
        <div className="paper-edition">
          <div className="paper-edition-identity">
            <img src={treeworkIcon} alt="" />
            <span>TreeWork Research</span>
          </div>
          <div className="paper-edition-tags" aria-label="Paper metadata">
            <span>
              <BookOpen size={12} />
              System paper
            </span>
            <span>{PAPER_VERSION}</span>
            <span>2026</span>
          </div>
        </div>
        <article
          ref={articleRef}
          className="paper-article"
          dangerouslySetInnerHTML={paperMarkup}
        />
      </main>

      <ReadingSettings
        open={settingsOpen}
        textScale={textScale}
        measure={measure}
        theme={theme}
        onClose={() => setSettingsOpen(false)}
        onTextScaleChange={setTextScale}
        onMeasureChange={setMeasure}
        onThemeChange={setTheme}
      />
      <SearchDialog
        open={searchOpen}
        article={articleRef.current}
        headings={headings}
        onClose={() => setSearchOpen(false)}
        onNavigate={navigateTo}
      />

      {citationOpen && (
        <div className="modal-layer" role="presentation" onMouseDown={() => setCitationOpen(false)}>
          <section
            className="citation-dialog"
            role="dialog"
            aria-modal="true"
            aria-label="Cite this paper"
            onMouseDown={(event) => event.stopPropagation()}
          >
            <header>
              <h2>Cite TreeWork</h2>
              <button className="icon-button" type="button" aria-label="Close citation" onClick={() => setCitationOpen(false)}>
                <X size={17} />
              </button>
            </header>
            <p>{CITATION_TEXT}</p>
            <div className="citation-actions">
              <button type="button" onClick={() => copyCitation("text")}>
                {copied === "text" ? <Check size={15} /> : <Quote size={15} />}
                Plain text
              </button>
              <button type="button" onClick={() => copyCitation("bibtex")}>
                {copied === "bibtex" ? <Check size={15} /> : <BookOpen size={15} />}
                BibTeX
              </button>
            </div>
          </section>
        </div>
      )}

      {lightbox && (
        <div className="lightbox" role="dialog" aria-modal="true" aria-label="Figure preview" onMouseDown={() => setLightbox(null)}>
          <header>
            <span><ZoomIn size={16} /> Figure preview</span>
            <button className="icon-button" type="button" aria-label="Close figure preview" onClick={() => setLightbox(null)}>
              <X size={18} />
            </button>
          </header>
          <img src={lightbox.src} alt={lightbox.alt} onMouseDown={(event) => event.stopPropagation()} />
        </div>
      )}
    </div>
  );
}
