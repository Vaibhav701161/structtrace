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
    const parsed: unknown = JSON.parse(content);
    return Array.isArray(parsed) ? parsed : [parsed];
  }
  if (format === "csv") return parseCsv(content);
  return content.split(/\r?\n/).filter((line) => line.trim()).map((line, index) => {
    try { return JSON.parse(line); }
    catch { throw new Error(`Line ${index + 1} is not valid JSON.`); }
  });
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
    try { return JSON.parse(trimmed); } catch { return value; }
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
  return parseRows(source.content, source.format).map((row) => valueAt(row, pointer)).filter((value) => value !== undefined);
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
    return {
      pointer,
      kind,
      enabled: false,
      expectedCoverage: coverage(expectedObjects),
      baselineCoverage: coverage(baselineObjects),
      candidateCoverage: coverage(candidateObjects),
      observedType: type,
      keys: type === "array" ? "/id" : undefined,
    };
  });
}

export function inferPointer(pointers: string[], candidates: string[], fallback: string) {
  return candidates.find((pointer) => pointers.includes(pointer)) ?? fallback;
}
