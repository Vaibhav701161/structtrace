import { describe, expect, it } from "vitest";
import { detectFormat, discoverRules, parseArtifact, parseRows, valueAt } from "../src/features/import/inspect";
import { exactJsonStringify, isExactJsonNumber, strictJsonParse } from "../src/lib/lossless-json";

describe("local import inspection", () => {
  it("reports the exact invalid JSONL line", () => {
    const artifact = parseArtifact("dataset", "cases.jsonl", '{"id":"a"}\nnot-json\n');
    expect(artifact.status).toBe("error");
    expect(artifact.message).toContain("Line 2");
  });

  it("suggests a compound invoice item key and numeric field semantics", () => {
    const row = '{"id":"a","expected":{"line_items":[{"description":"Paper","quantity":2,"unit_price":"40.00","amount":"80.00"}]}}\n';
    const output = '{"id":"a","output":{"line_items":[{"description":"Paper","quantity":2,"unit_price":"40.00","amount":"80.00"}]}}\n';
    const rules = discoverRules(parseArtifact("dataset", "data.jsonl", row), parseArtifact("baseline", "base.jsonl", output), parseArtifact("candidate", "next.jsonl", output), "/expected", "/output", "/output");
    const items = rules.find((rule) => rule.pointer === "/line_items");
    expect(items?.keyFields).toEqual(["/description", "/unit_price"]);
    expect(items?.arrayFields).toContainEqual({ pointer: "/quantity", kind: "exact_integer" });
    expect(items?.arrayFields).toContainEqual({ pointer: "/amount", kind: "decimal_tolerance" });
  });

  it("rejects duplicate JSON keys instead of silently keeping the last value", () => {
    const artifact = parseArtifact("candidate", "candidate.jsonl", '{"id":"a","output":{"answer":4,"answer":5}}\n');
    expect(artifact.status).toBe("error");
    expect(artifact.message).toContain("Line 1");
    expect(artifact.message).toContain('Duplicate object key "answer"');
  });

  it("parses quoted CSV with nested JSON deterministically", () => {
    const rows = parseRows('id,output\na,"{""answer"":4}"\n', "csv");
    const answer = valueAt(rows[0], "/output/answer");
    expect(isExactJsonNumber(answer)).toBe(true);
    expect(exactJsonStringify(answer)).toBe("4");
  });

  it("preserves arbitrary precision integers and decimals exactly", () => {
    const value = strictJsonParse('{"integer":9007199254740993,"decimal":0.12345678901234567890123456789}');
    expect(exactJsonStringify(value)).toBe('{"integer":9007199254740993,"decimal":0.12345678901234567890123456789}');
  });

  it("treats dangerous JSON names as plain own properties", () => {
    const value = strictJsonParse('{"__proto__":{"polluted":true},"constructor":1,"prototype":2}') as Record<string, unknown>;
    expect(Object.getPrototypeOf(value)).toBeNull();
    expect(Object.hasOwn(value, "__proto__")).toBe(true);
    expect(valueAt(value, "/__proto__/polluted")).toBe(true);
    expect(valueAt({}, "/toString")).toBeUndefined();
  });

  it("detects arrays as JSON and newline records as JSONL", () => {
    expect(detectFormat("cases.data", "[{}]")).toBe("json");
    expect(detectFormat("cases.data", "{}\n{}\n")).toBe("jsonl");
  });

  it("suggests numeric-string and keyed-array semantics without enabling them", () => {
    const dataset = parseArtifact("dataset", "data.jsonl", '{"id":"a","expected":{"amount":"12.50","items":[{"sku":"x"}]}}\n');
    const baseline = parseArtifact("baseline", "baseline.jsonl", '{"id":"a","output":{"amount":"12.50","items":[{"sku":"x"}]}}\n');
    const candidate = parseArtifact("candidate", "candidate.jsonl", '{"id":"a","output":{"amount":"12.50","items":[{"sku":"x"}]}}\n');
    const rules = discoverRules(dataset, baseline, candidate, "/expected", "/output", "/output");
    expect(rules.find((rule) => rule.pointer === "/amount")).toMatchObject({ kind: "decimal_exact", enabled: false });
    expect(rules.find((rule) => rule.pointer === "/items")).toMatchObject({ kind: "keyed_array", enabled: false });
    expect(rules.find((rule) => rule.pointer === "/items")).toMatchObject({ keyFields: ["/sku"] });
  });
});
