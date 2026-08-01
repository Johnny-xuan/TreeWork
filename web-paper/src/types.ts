export interface PaperHeading {
  id: string;
  level: number;
  number: string;
  title: string;
}

export interface PaperSection extends PaperHeading {
  children: PaperHeading[];
}

export interface SearchResult {
  id: string;
  section: string;
  excerpt: string;
}

export type ReaderTheme = "light" | "dark" | "system";
export type ReadingMeasure = "compact" | "standard" | "wide";
