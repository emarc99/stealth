import { describe, expect, it, vi, beforeEach } from "vitest";
import { z } from "zod";
import { createRouteHandler } from "../../../src/server/api/handler";
import * as metrics from "../../../src/server/api/metrics";
import { ApiError } from "../../../src/server/api/errors";
import { MemoryApiRepository } from "../../../src/server/api/memory-repository";
import { ACTOR_HEADER } from "../../../src/server/api/actor";

vi.mock("../../../src/server/api/metrics", () => ({
  incrementCounter: vi.fn(),
  recordHistogram: vi.fn(),
}));

describe("createRouteHandler", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    const globalApi = globalThis as any;
    globalApi.__stealthApiRepository = undefined;
  });

  it("executes handler and logs success metrics", async () => {
    const handler = createRouteHandler({
      handler: () => new Response(JSON.stringify({ ok: true }), { status: 200 }),
    });

    const request = new Request("http://localhost/api/test", { method: "GET" });
    const response = await handler(request);

    expect(response.status).toBe(200);
    expect(metrics.incrementCounter).toHaveBeenCalledWith("api_requests_total", {
      method: "GET",
      path: "/api/test",
      status: "200",
    });
    expect(metrics.recordHistogram).toHaveBeenCalledWith(
      "api_latency",
      expect.any(Number),
      { method: "GET", path: "/api/test", status: "200" }
    );
  });

  it("catches errors and logs failure metrics", async () => {
    const handler = createRouteHandler({
      handler: () => {
        throw new ApiError(400, "bad_request", "Custom error");
      },
    });

    const request = new Request("http://localhost/api/test", { method: "POST" });
    const response = await handler(request);

    expect(response.status).toBe(400);
    expect(metrics.incrementCounter).toHaveBeenCalledWith("api_errors_total", {
      method: "POST",
      path: "/api/test",
      status: "400",
    });
  });

  it("validates body schema", async () => {
    const handler = createRouteHandler({
      bodySchema: z.object({ value: z.number() }),
      handler: ({ body }) => {
        expect(body.value).toBe(42);
        return new Response("OK");
      },
    });

    const request = new Request("http://localhost/api/test", {
      method: "POST",
      body: JSON.stringify({ value: 42 }),
      headers: { "Content-Type": "application/json" }
    });
    const response = await handler(request);
    expect(response.status).toBe(200);
  });

  it("blocks invalid body schema", async () => {
    const handler = createRouteHandler({
      bodySchema: z.object({ value: z.number() }),
      handler: () => new Response("OK"),
    });

    const request = new Request("http://localhost/api/test", {
      method: "POST",
      body: JSON.stringify({ value: "not a number" }),
      headers: { "Content-Type": "application/json" }
    });
    const response = await handler(request);
    expect(response.status).toBe(422);
  });

  it("authenticates the actor", async () => {
    const handler = createRouteHandler({
      requireAuth: true,
      handler: ({ actorId }) => {
        expect(actorId).toBe("GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB");
        return new Response("OK");
      },
    });

    const request = new Request("http://localhost/api/test", {
      method: "GET",
      headers: { [ACTOR_HEADER]: "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB" }
    });
    const response = await handler(request);
    expect(response.status).toBe(200);
  });

  it("blocks unauthenticated requests when auth is required", async () => {
    const handler = createRouteHandler({
      requireAuth: true,
      handler: () => new Response("OK"),
    });

    const request = new Request("http://localhost/api/test", { method: "GET" });
    const response = await handler(request);
    expect(response.status).toBe(401);
  });

  it("adds Cache-Control headers if configured", async () => {
    const handler = createRouteHandler({
      cacheSeconds: 60,
      handler: () => new Response("OK", { status: 200 }),
    });

    const request = new Request("http://localhost/api/test", { method: "GET" });
    const response = await handler(request);
    expect(response.status).toBe(200);
    expect(response.headers.get("Cache-Control")).toBe("public, max-age=60");
  });
});
