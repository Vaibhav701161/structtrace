import { describe, expect, it } from "vitest";
import { draftForStorage } from "../src/api/client";

describe("draft persistence boundary", () => {
  it("sends staged references without source contents", () => {
    const stored = draftForStorage({
      name: "comparison",
      sources: { dataset: { sourceId: "source-1", hash: "digest", content: "private bytes" } },
    });
    expect(stored).toEqual({
      name: "comparison",
      sources: { dataset: { sourceId: "source-1", hash: "digest" } },
    });
    expect(JSON.stringify(stored)).not.toContain("private bytes");
  });
});
