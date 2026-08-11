import type { FieldRule, SourceArtifact, SourceKind } from "../../api/types";

export function detectFormat(name: string, content: string): SourceArtifact["format"] {
  const lower = name.toLowerCase();
  if (lower.endsWith(".csv")) return "csv";
  if (lower.endsWith(".jsonl") || lower.endsWith(".ndjson")) return "jsonl";
  const trimmed = content.trimStart();
  return trimmed.startsWith("[") ? "json" : "jsonl";
}

export function parseArtifact(kind: SourceKind, name: string, content: string): SourceArtifact {
  const format = kind === "schema" ? "json" : detectFormat(name, content);
  try {
    const rows = parseRows(content, format).length;
    if (rows === 0) throw new Error("The file contains no records.");
    return { kind, name, format, content, bytes: new Blob([content]).size, rows, status: "ready", sourceId: "", hash: "" };
  } catch (error) {
    return {
      kind, name, format, content, bytes: new Blob([content]).size, rows: 0, status: "error",
      message: error instanceof Error ? error.message : "This file could not be read.", sourceId: "", hash: "",
    };
  }
}

export function parseRows(content: string, format: SourceArtifact["format"]): unknown[] {
  if (format === "json") {
    const parsed = strictJsonParse(content);
    return Array.isArray(parsed) ? parsed : [parsed];
  }
  if (format === "csv") return parseCsv(content);
  return content.split(/\r?\n/).filter((line) => line.trim()).map((line, index) => {
    try { return strictJsonParse(line); }
    catch (error) {
      const detail = error instanceof Error ? ` ${error.message}` : "";
      throw new Error(`Line ${index + 1} is not valid JSON.${detail}`);
    }
  });
}

/**
 * Parse JSON while rejecting duplicate object keys. JavaScript's JSON.parse
 * silently keeps the final value, whereas StructTrace's Rust parser rejects
 * ambiguous objects. Import inspection must apply the same trust boundary.
 */
export function strictJsonParse(content: string): unknown {
  class Parser {
    private index = 0;

    constructor(private readonly source: string) {}

    parse(): unknown {
      const value = this.value();
      this.space();
      if (this.index !== this.source.length) this.fail("Unexpected trailing content.");
      return value;
    }

    private value(): unknown {
      this.space();
      const char = this.source[this.index];
      if (char === "{") return this.object();
      if (char === "[") return this.array();
      if (char === '"') return this.string();
      if (char === "t") return this.literal("true", true);
      if (char === "f") return this.literal("false", false);
      if (char === "n") return this.literal("null", null);
      if (char === "-" || (char >= "0" && char <= "9")) return this.number();
      this.fail("Expected a JSON value.");
    }

    private object(): Record<string, unknown> {
      this.index += 1;
      const result: Record<string, unknown> = {};
      const keys = new Set<string>();
      this.space();
      if (this.take("}")) return result;
      while (true) {
        this.space();
        if (this.source[this.index] !== '"') this.fail("Expected an object key.");
        const key = this.string();
        if (keys.has(key)) this.fail(`Duplicate object key ${JSON.stringify(key)}.`);
        keys.add(key);
        this.space();
        if (!this.take(":")) this.fail("Expected ':' after an object key.");
        result[key] = this.value();
        this.space();
        if (this.take("}")) return result;
        if (!this.take(",")) this.fail("Expected ',' or '}' in an object.");
      }
    }

    private array(): unknown[] {
      this.index += 1;
      const result: unknown[] = [];
      this.space();
      if (this.take("]")) return result;
      while (true) {
        result.push(this.value());
        this.space();
        if (this.take("]")) return result;
        if (!this.take(",")) this.fail("Expected ',' or ']' in an array.");
      }
    }

    private string(): string {
      const start = this.index;
      this.index += 1;
      while (this.index < this.source.length) {
        const char = this.source[this.index];
        if (char === '"') {
          this.index += 1;
          return JSON.parse(this.source.slice(start, this.index)) as string;
        }
        if (char === "\\") {
          this.index += 2;
          continue;
        }
        if (char.charCodeAt(0) < 0x20) this.fail("Unescaped control character in a string.");
        this.index += 1;
      }
      this.fail("Unterminated JSON string.");
    }

    private number(): number {
      const rest = this.source.slice(this.index);
      const match = rest.match(/^-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/);
      if (!match) this.fail("Invalid JSON number.");
      this.index += match[0].length;
      const value = Number(match[0]);
      if (!Number.isFinite(value)) this.fail("JSON number is outside the supported range.");
      return value;
    }

    private literal<T>(text: string, value: T): T {
      if (!this.source.startsWith(text, this.index)) this.fail(`Expected '${text}'.`);
      this.index += text.length;
      return value;
    }

    private take(char: string): boolean {
      if (this.source[this.index] !== char) return false;
      this.index += 1;
      return true;
    }

    private space(): void {
      while (/\s/.test(this.source[this.index] ?? "")) this.index += 1;
    }

    private fail(message: string): never {
      throw new Error(`${message} (byte ${new TextEncoder().encode(this.source.slice(0, this.index)).length})`);
    }
  }

  return new Parser(content).parse();
}

function parseCsv(content: string): Record<string, unknown>[] {
  const lines = content.split(/\r?\n/).filter((line) => line.trim());
  if (lines.length < 2) throw new Error("CSV needs a header and at least one data row.");
  const split = (line: string) => {
    const output: string[] = [];
    let value = "";
    let quoted = false;
    for (let index = 0; index < line.length; index += 1) {
      const char = line[index];
      if (char === '"' && line[index + 1] === '"' && quoted) { value += '"'; index += 1; }
      else if (char === '"') quoted = !quoted;
      else if (char === "," && !quoted) { output.push(value); value = ""; }
      else value += char;
    }
    output.push(value);
    if (quoted) throw new Error("CSV contains an unterminated quoted value.");
    return output;
  };
  const headers = split(lines[0]);
  return lines.slice(1).map((line, rowIndex) => {
    const values = split(line);
    if (values.length !== headers.length) throw new Error(`CSV row ${rowIndex + 2} has ${values.length} columns; expected ${headers.length}.`);
    return Object.fromEntries(headers.map((header, index) => [header, parseCell(values[index])]));
  });
}

function parseCell(value: string): unknown {
  const trimmed = value.trim();
  if (!trimmed) return "";
  if (trimmed === "true") return true;
  if (trimmed === "false") return false;
  if (trimmed === "null") return null;
  if (/^-?(0|[1-9]\d*)(\.\d+)?$/.test(trimmed)) return Number(trimmed);
  if ((trimmed.startsWith("{") && trimmed.endsWith("}")) || (trimmed.startsWith("[") && trimmed.endsWith("]"))) {
    try { return strictJsonParse(trimmed); } catch { return value; }
  }
  return value;
}

export function valueAt(value: unknown, pointer: string): unknown {
  if (!pointer || pointer === "/") return value;
  return pointer.split("/").slice(1).reduce<unknown>((current, segment) => {
    if (current === null || typeof current !== "object") return undefined;
    const key = segment.replace(/~1/g, "/").replace(/~0/g, "~");
    return (current as Record<string, unknown>)[key];
  }, value);
}

export function pointerCandidates(rows: unknown[]): string[] {
  const found = new Set<string>();
  const visit = (value: unknown, path: string, depth: number) => {
    if (depth > 5 || value === null || typeof value !== "object") return;
    if (Array.isArray(value)) {
      if (value.length) visit(value[0], path, depth + 1);
      return;
    }
    Object.entries(value as Record<string, unknown>).forEach(([key, child]) => {
      const pointer = `${path}/${key.replace(/~/g, "~0").replace(/\//g, "~1")}`;
      found.add(pointer);
      visit(child, pointer, depth + 1);
    });
  };
  rows.slice(0, 25).forEach((row) => visit(row, "", 0));
  return [...found].sort();
}

function outputValues(source: SourceArtifact | undefined, pointer: string): unknown[] {
  if (!source || source.status !== "ready") return [];
  return (source.preview ?? parseRows(source.content, source.format)).map((row) => valueAt(row, pointer)).filter((value) => value !== undefined);
}

function leafPointers(value: unknown, path = "", output = new Set<string>()): Set<string> {
  if (value === null || typeof value !== "object") {
    if (path) output.add(path);
    return output;
  }
  if (Array.isArray(value)) {
    if (path) output.add(path);
    return output;
  }
  for (const [key, child] of Object.entries(value as Record<string, unknown>)) {
    leafPointers(child, `${path}/${key.replace(/~/g, "~0").replace(/\//g, "~1")}`, output);
  }
  return output;
}

function keyedItemRules(values: unknown[]): Pick<FieldRule, "keyFields" | "arrayFields"> {
  const items = values.flatMap((value) => Array.isArray(value) ? value.slice(0, 25) : []);
  const pointers = new Set<string>();
  items.forEach((item) => leafPointers(item, "", pointers));
  const candidates = [...pointers].sort().map((pointer) => {
    const observed = items.map((item) => valueAt(item, pointer)).filter((value) => value !== undefined && value !== null);
    const lower = pointer.toLowerCase();
    const integerLike = observed.length > 0 && observed.every((value) => (typeof value === "number" && Number.isInteger(value)) || (typeof value === "string" && /^-?(0|[1-9]\d*)$/.test(value)));
    const decimalLike = observed.length > 0 && observed.every((value) => typeof value === "number" || (typeof value === "string" && /^-?(0|[1-9]\d*)(\.\d+)?$/.test(value)));
    const kind = integerLike
      ? "exact_integer"
      : decimalLike
        ? "decimal_tolerance"
        : /(date|_at$)/.test(lower)
          ? "canonical_date"
          : /(name|title|description|label|text)/.test(lower)
            ? "normalized_string"
            : "exact";
    return { pointer, kind } as NonNullable<FieldRule["arrayFields"]>[number];
  });
  const identity = candidates.find(({ pointer }) => /\/(id|sku|product_code|code)$/i.test(pointer))?.pointer;
  const compoundInvoiceKey = ["/description", "/unit_price"].filter((pointer) => candidates.some((candidate) => candidate.pointer === pointer));
  const preferredKeys = identity ? [identity] : compoundInvoiceKey.length === 2 ? compoundInvoiceKey : [candidates[0]?.pointer ?? "/id"];
  return {
    keyFields: preferredKeys,
    arrayFields: candidates.filter(({ pointer }) => !preferredKeys.includes(pointer)),
  };
}

export function discoverRules(
  dataset: SourceArtifact,
  baseline: SourceArtifact,
  candidate: SourceArtifact,
  expectedPointer: string,
  baselinePointer: string,
  candidatePointer: string,
): FieldRule[] {
  const expectedObjects = outputValues(dataset, expectedPointer);
  const baselineObjects = outputValues(baseline, baselinePointer);
  const candidateObjects = outputValues(candidate, candidatePointer);
  const pointers = new Set<string>();
  [...expectedObjects, ...baselineObjects, ...candidateObjects].forEach((value) => leafPointers(value, "", pointers));
  const total = Math.max(expectedObjects.length, baselineObjects.length, candidateObjects.length, 1);
  return [...pointers].sort().map((pointer) => {
    const values = [...expectedObjects, ...baselineObjects, ...candidateObjects]
      .map((value) => valueAt(value, pointer)).filter((value) => value !== undefined && value !== null);
    const type = values.length ? (Array.isArray(values[0]) ? "array" : typeof values[0]) : "unknown";
    const lower = pointer.toLowerCase();
    const numericStrings = type === "string" && values.every((value) => typeof value === "string" && /^-?(0|[1-9]\d*)(\.\d+)?$/.test(value));
    const integerStrings = numericStrings && values.every((value) => typeof value === "string" && /^-?(0|[1-9]\d*)$/.test(value));
    const kind: FieldRule["kind"] = type === "array" ? "keyed_array" : type === "number"
      ? values.every(Number.isInteger) ? "exact_integer" : "decimal_exact"
      : numericStrings ? integerStrings ? "exact_integer" : "decimal_exact"
      : lower.includes("date") ? "canonical_date"
        : type === "string" && /(name|title|description|label|text)/.test(lower) ? "normalized_string"
          : "exact";
    const coverage = (objects: unknown[]) => objects.filter((value) => valueAt(value, pointer) !== undefined).length / total;
    const keyed = type === "array" ? keyedItemRules(values) : {};
    return {
      pointer,
      kind,
      enabled: false,
      expectedCoverage: coverage(expectedObjects),
      baselineCoverage: coverage(baselineObjects),
      candidateCoverage: coverage(candidateObjects),
      observedType: type,
      keys: type === "array" ? "/id" : undefined,
      ...keyed,
    };
  });
}

export function inferPointer(pointers: string[], candidates: string[], fallback: string) {
  return candidates.find((pointer) => pointers.includes(pointer)) ?? fallback;
}
