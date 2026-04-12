// ---- Shared types ----

export interface TableColumn {
  id: string;
  name: string;
  type?: string;
}

export interface TableRow {
  id: string;
  cells: Record<string, string>;
}

export interface CustomParserDef {
  id: string;
  name: string;
  pattern: string;
  columns: string[];
}

export interface TableContent {
  columns: TableColumn[];
  rows: TableRow[];
  customParsers?: CustomParserDef[];
}

// ---- Import parser interface ----

export interface ParseResult {
  columns: string[];
  rows: Record<string, string>[];
}

export interface ImportParser {
  id: string;
  name: string;
  description: string;
  icon: string;
  parse(input: string, options?: Record<string, unknown>): ParseResult;
}

// ---- Helpers ----

export function slugifyColumn(name: string, existing: string[]): string {
  let base = name
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");
  if (!base) base = "col";
  let id = base;
  let n = 2;
  while (existing.includes(id)) {
    id = `${base}-${n}`;
    n++;
  }
  return id;
}

function normalizeUrl(raw: string): string {
  return raw.startsWith("~/") || raw.startsWith("-/") ? "https://" + raw.slice(2) : raw;
}

// ---- CSV / PSV shared logic ----

function parseSeparated(
  input: string,
  separator: string,
  options?: Record<string, unknown>,
): ParseResult {
  const lines = input.split("\n").filter((l) => l.trim().length > 0);
  if (lines.length === 0) return { columns: [], rows: [] };

  const splitLine = (line: string): string[] => {
    if (separator === ",") return parseCsvLine(line);
    return line.split(separator).map((c) => c.trim());
  };

  const allFields = lines.map(splitLine);
  const colCount = allFields[0].length;

  // Determine if first row is header
  let hasHeader = options?.hasHeader as boolean | undefined;
  if (hasHeader === undefined && allFields.length > 1) {
    // Auto-detect: if first row is all non-numeric and rest have at least one numeric
    const firstAllText = allFields[0].every((v) => isNaN(Number(v)) || v.trim() === "");
    const restHasNumeric = allFields.slice(1).some((row) =>
      row.some((v) => v.trim() !== "" && !isNaN(Number(v))),
    );
    hasHeader = firstAllText && restHasNumeric;
  }

  let columns: string[];
  let dataRows: string[][];

  if (hasHeader) {
    columns = allFields[0].map((h) => h.trim());
    dataRows = allFields.slice(1);
  } else {
    columns = Array.from({ length: colCount }, (_, i) => `col${i + 1}`);
    dataRows = allFields;
  }

  const rows = dataRows.map((fields) => {
    const row: Record<string, string> = {};
    for (let i = 0; i < columns.length; i++) {
      row[columns[i]] = fields[i]?.trim() ?? "";
    }
    return row;
  });

  return { columns, rows };
}

function parseCsvLine(line: string): string[] {
  const fields: string[] = [];
  let current = "";
  let inQuotes = false;

  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (inQuotes) {
      if (ch === '"') {
        if (i + 1 < line.length && line[i + 1] === '"') {
          current += '"';
          i++;
        } else {
          inQuotes = false;
        }
      } else {
        current += ch;
      }
    } else {
      if (ch === '"') {
        inQuotes = true;
      } else if (ch === ",") {
        fields.push(current.trim());
        current = "";
      } else {
        current += ch;
      }
    }
  }
  fields.push(current.trim());
  return fields;
}

// ---- Built-in parsers ----

export const csvParser: ImportParser = {
  id: "csv",
  name: "CSV",
  description: "Comma-separated values",
  icon: "table_rows",
  parse: (input, options) => parseSeparated(input, ",", options),
};

export const psvParser: ImportParser = {
  id: "psv",
  name: "PSV",
  description: "Pipe-separated values",
  icon: "view_column",
  parse: (input, options) => parseSeparated(input, "|", options),
};

export const jsonParser: ImportParser = {
  id: "json",
  name: "JSON",
  description: "Array of objects",
  icon: "data_object",
  parse(input: string): ParseResult {
    let data: unknown;
    try {
      data = JSON.parse(input);
    } catch {
      return { columns: [], rows: [] };
    }
    if (!Array.isArray(data)) return { columns: [], rows: [] };

    const colSet = new Set<string>();
    const rows: Record<string, string>[] = [];

    for (const item of data) {
      if (typeof item !== "object" || item === null) continue;
      const row: Record<string, string> = {};
      for (const [key, val] of Object.entries(item)) {
        colSet.add(key);
        row[key] = val == null ? "" : String(val);
      }
      rows.push(row);
    }

    const columns = Array.from(colSet);
    // Fill missing keys with empty string
    for (const row of rows) {
      for (const col of columns) {
        if (!(col in row)) row[col] = "";
      }
    }

    return { columns, rows };
  },
};

export const kvParser: ImportParser = {
  id: "kv",
  name: "Key-Value",
  description: "key=value or key: value pairs",
  icon: "assignment",
  parse(input: string): ParseResult {
    const lines = input.split("\n").map((l) => l.trim()).filter((l) => l.length > 0);
    const KV_RE = /^([^=:]+?)\s*[=:]\s*(.*)$/;

    const columns: string[] = [];
    const row: Record<string, string> = {};

    for (const line of lines) {
      const m = KV_RE.exec(line);
      if (!m) continue;
      const key = m[1].trim();
      const val = m[2].trim();
      if (!columns.includes(key)) columns.push(key);
      row[key] = val;
    }

    if (columns.length === 0) return { columns: [], rows: [] };
    return { columns, rows: [row] };
  },
};

const URL_GLOBAL_RE = /(?:https?:\/\/|[~\-]\/)[^\s]+/g;

export const urlDescParser: ImportParser = {
  id: "url-desc",
  name: "URL + Description",
  description: "Extract URLs with surrounding text",
  icon: "link",
  parse(input: string): ParseResult {
    const columns = ["url", "desc"];
    const text = input.trim();
    if (!text) return { columns, rows: [] };

    // Find all URL matches with positions
    const matches: { url: string; start: number; end: number }[] = [];
    let m: RegExpExecArray | null;
    // Reset lastIndex for global regex
    URL_GLOBAL_RE.lastIndex = 0;
    while ((m = URL_GLOBAL_RE.exec(text)) !== null) {
      matches.push({ url: m[0], start: m.index, end: m.index + m[0].length });
    }

    // No URLs found: single row with all text as desc
    if (matches.length === 0) {
      const desc = text.replace(/\s+/g, " ").trim();
      return { columns, rows: [{ url: "", desc }] };
    }

    // URL[0] gets: text before it + text after it (until URL[1]).
    // URL[i>0] gets: text after it (until URL[i+1] or end). Between-text belongs to preceding URL.
    const rows: Record<string, string>[] = [];

    for (let i = 0; i < matches.length; i++) {
      const match = matches[i];
      const prefix = i === 0 ? text.slice(0, match.start) : "";
      const suffixEnd = i + 1 < matches.length ? matches[i + 1].start : text.length;
      const suffix = text.slice(match.end, suffixEnd);

      const desc = (prefix + " " + suffix).replace(/\s+/g, " ").trim();
      rows.push({ url: normalizeUrl(match.url), desc });
    }

    return { columns, rows };
  },
};

// ---- Custom regex parser factory ----

export function makeCustomParser(def: CustomParserDef): ImportParser {
  return {
    id: def.id,
    name: def.name,
    description: `Custom regex: ${def.pattern}`,
    icon: "code",
    parse(input: string): ParseResult {
      const re = new RegExp(def.pattern);
      const lines = input.split("\n").map((l) => l.trim()).filter((l) => l.length > 0);

      const rows = lines.map((line) => {
        const m = re.exec(line);
        const row: Record<string, string> = {};
        for (const col of def.columns) {
          row[col] = m?.groups?.[col] ?? "";
        }
        return row;
      });

      return { columns: [...def.columns], rows };
    },
  };
}

// ---- Registry ----

export const builtinImportParsers: ImportParser[] = [
  csvParser,
  psvParser,
  jsonParser,
  kvParser,
  urlDescParser,
];
