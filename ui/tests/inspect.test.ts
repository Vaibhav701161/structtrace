import { describe, expect, it } from "vitest";
import { detectFormat, parseArtifact, parseRows, valueAt } from "../src/features/import/inspect";

describe("local import inspection", () => {
  it("reports the exact invalid JSONL line", () => {
    const artifact = parseArtifact("dataset", "cases.jsonl", '{"id":"a"}\nnot-json\n');
    expect(artifact.status).toBe("error");
    expect(artifact.message).toContain("Line 2");
  });

  it("parses quoted CSV with nested JSON deterministically", () => {
    const rows = parseRows('id,output\na,"{""answer"":4}"\n', "csv");
    expect(valueAt(rows[0], "/output/answer")).toBe(4);
  });

  it("detects arrays as JSON and newline records as JSONL", () => {
    expect(detectFormat("cases.data", "[{}]")).toBe("json");
    expect(detectFormat("cases.data", "{}\n{}\n")).toBe("jsonl");
  });
});
