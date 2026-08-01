/// <reference types="vite/client" />

declare module "katex/contrib/auto-render" {
  interface AutoRenderOptions {
    delimiters?: Array<{
      left: string;
      right: string;
      display: boolean;
    }>;
    ignoredClasses?: string[];
    throwOnError?: boolean;
    strict?: boolean | "ignore" | "warn" | "error";
  }

  export default function renderMathInElement(
    element: HTMLElement,
    options?: AutoRenderOptions,
  ): void;
}
