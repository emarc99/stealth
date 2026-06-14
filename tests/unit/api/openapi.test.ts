import { describe, expect, it } from "vitest";

import { openApiDocument } from "../../../src/server/api/openapi";

describe("OpenAPI document", () => {
  it("publishes every v1 endpoint family", () => {
    expect(Object.keys(openApiDocument.paths)).toEqual(
      expect.arrayContaining([
        "/health",
        "/policies/{owner}",
        "/policies/evaluate",
        "/postage",
        "/postage/{messageId}/settle",
        "/receipts",
        "/receipts/{messageId}/read",
      ]),
    );
  });

  it("marks mutating owner operations with actor security", () => {
    expect(openApiDocument.paths["/policies/{owner}"].put.security).toEqual([{ ActorHeader: [] }]);
  });
});
