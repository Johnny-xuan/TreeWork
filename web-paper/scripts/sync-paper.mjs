import { spawnSync } from "node:child_process";
import { access, mkdir, readFile, writeFile } from "node:fs/promises";
import { constants } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { load } from "cheerio";

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const packageDirectory = path.resolve(scriptDirectory, "..");
const repositoryDirectory = path.resolve(packageDirectory, "..");
const paperDirectory = path.join(repositoryDirectory, "paper");
const outputDirectory = path.join(packageDirectory, "src", "generated");
const outputPath = path.join(outputDirectory, "paper.html");

async function executable(command) {
  if (command.includes(path.sep)) {
    try {
      await access(command, constants.X_OK);
      return true;
    } catch {
      return false;
    }
  }
  const result = spawnSync("sh", ["-c", `command -v ${command}`], {
    stdio: "ignore",
  });
  return result.status === 0;
}

async function resolvePandoc() {
  const candidates = [
    process.env.PANDOC,
    "pandoc",
    "/opt/anaconda3/bin/pandoc",
    "/usr/local/bin/pandoc",
    "/opt/homebrew/bin/pandoc",
  ].filter(Boolean);
  for (const candidate of candidates) {
    if (await executable(candidate)) {
      return candidate;
    }
  }
  throw new Error(
    "Pandoc is required to sync the Web Paper. Install pandoc or set PANDOC.",
  );
}

function normalizePaper(documentHtml, abstractHtml) {
  const document = load(documentHtml);
  const fragment = document("body").html() ?? documentHtml;
  const $ = load(`<article id="paper-content">${fragment}</article>`, null, false);
  const article = $("#paper-content");
  const equationNumbers = new Map();
  const theoremNumbers = new Map();
  let equationNumber = 0;
  let theoremSection = "";
  let theoremNumber = 0;

  article.find("span.math").each((_, element) => {
    const math = $(element);
    const source = math.html() ?? "";
    const match = source.match(/\\label\{([^}]+)\}/);
    if (!match) {
      return;
    }
    const label = match[1];
    math.html(source.replace(/\s*\\label\{[^}]+\}/g, ""));
    const block = math.closest("p");
    if (block.length) {
      block.attr("id", label);
      if (label.startsWith("eq:")) {
        equationNumber += 1;
        equationNumbers.set(label, equationNumber);
        block.addClass("equation-block");
        block.append(
          `<a class="equation-number" href="#${label}" aria-label="Equation ${equationNumber}">(${equationNumber})</a>`,
        );
      }
    }
  });

  for (const [label, number] of equationNumbers) {
    article
      .find(`a[data-reference="${label}"]`)
      .text(`(${number})`);
  }

  article
    .find("h1, .definition, .theorem, .lemma, .proposition, .corollary")
    .each((_, element) => {
      const node = $(element);
      if (element.tagName === "h1") {
        theoremSection = node
          .find(".header-section-number, .section-number")
          .first()
          .text()
          .trim();
        theoremNumber = 0;
        return;
      }
      theoremNumber += 1;
      const label = theoremSection
        ? `${theoremSection}.${theoremNumber}`
        : String(theoremNumber);
      const strong = node.find("p:first-child > strong:first-child");
      strong.text(
        strong
          .text()
          .replace(
            /^(Definition|Theorem|Lemma|Proposition|Corollary)\s+\d+(?:\.\d+)?/,
            (_, kind) => `${kind} ${label}`,
          ),
      );
      const id = node.attr("id");
      if (id) {
        theoremNumbers.set(id, label);
      }
    });

  for (const [id, number] of theoremNumbers) {
    article.find(`a[data-reference="${id}"]`).text(number);
  }

  article.find("figure > img[id]").each((_, image) => {
    const node = $(image);
    node.parent().attr("id", node.attr("id"));
    node.removeAttr("id");
  });

  article.find("img[src^='assets/']").each((_, image) => {
    const node = $(image);
    const filename = path.basename(node.attr("src"));
    const dimensions = {
      "persistent_project_state_infographic_8k_ultra_clear.png": [8192, 4610],
      "two_panel_agent_workflow_comparison_4k_final.png": [4096, 2048],
    }[filename];
    node.attr("src", `__TREEWORK_PAPER_ASSET__/${filename}`);
    node.attr("loading", "lazy");
    node.attr("decoding", "async");
    if (dimensions) {
      node.attr("width", dimensions[0]);
      node.attr("height", dimensions[1]);
    }
  });

  article.find("a[href^='#']").each((_, anchor) => {
    $(anchor).attr("data-paper-link", "true");
  });

  article
    .find("#title-block-header")
    .after(
      `<section class="abstract" aria-labelledby="abstract"><h1 id="abstract">Abstract</h1>${abstractHtml}</section>`,
    );

  article.find("table").each((_, table) => {
    const node = $(table);
    node.wrap('<div class="table-scroll" role="region" tabindex="0"></div>');
  });

  if (!article.find("#references").length) {
    article
      .find("#refs")
      .before('<h1 id="references" class="unnumbered">References</h1>');
  }

  article.find("h1, h2, h3, h4").each((_, heading) => {
    const node = $(heading);
    if (!node.attr("id")) {
      return;
    }
    node.append(
      `<a class="heading-anchor" href="#${node.attr("id")}" aria-label="Link to this section">#</a>`,
    );
  });

  return article.html() ?? "";
}

const pandoc = await resolvePandoc();
const mainSource = await readFile(path.join(paperDirectory, "main.tex"), "utf8");
const abstractSource = mainSource.match(
  /\\begin\{abstract\}([\s\S]*?)\\end\{abstract\}/,
)?.[1];
if (!abstractSource) {
  throw new Error("paper/main.tex does not contain an abstract environment");
}
const abstractResult = spawnSync(
  pandoc,
  ["--from=latex", "--to=html5"],
  {
    input: abstractSource.replace(/\\system\b/g, "TreeWork"),
    encoding: "utf8",
    maxBuffer: 1024 * 1024,
  },
);
if (abstractResult.status !== 0) {
  throw new Error(abstractResult.stderr || "Pandoc failed to convert the abstract");
}
const result = spawnSync(
  pandoc,
  [
    "main.tex",
    "--from=latex",
    "--to=html5",
    "--mathjax",
    "--standalone",
    "--number-sections",
    "--citeproc",
    "--metadata=link-citations:true",
    "--bibliography=references.bib",
  ],
  {
    cwd: paperDirectory,
    encoding: "utf8",
    maxBuffer: 16 * 1024 * 1024,
  },
);

if (result.status !== 0) {
  throw new Error(result.stderr || `Pandoc failed with exit code ${result.status}`);
}

await mkdir(outputDirectory, { recursive: true });
await writeFile(
  outputPath,
  `${normalizePaper(result.stdout, abstractResult.stdout).trimEnd()}\n`,
  "utf8",
);
console.log(`Synced ${path.relative(repositoryDirectory, outputPath)} from paper/main.tex`);
