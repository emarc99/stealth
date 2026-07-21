import { z } from "zod";
import { requireActor } from "./actor";
import { getApiContext } from "./context";
import { checkAccountLimit, checkIpLimit } from "./abuse-service";
import { ApiError } from "./errors";
import { apiFailure, apiSuccess } from "./response";
import * as metrics from "./metrics";
import { parseJsonBody } from "./request";

export type RateLimitConfig = {
  type: "account" | "ip";
};

export type RouteConfig<
  BodySchema extends z.ZodTypeAny,
  QuerySchema extends z.ZodTypeAny,
  ParamsSchema extends z.ZodTypeAny
> = {
  requireAuth?: boolean;
  rateLimit?: RateLimitConfig;
  bodySchema?: BodySchema;
  querySchema?: QuerySchema;
  paramsSchema?: ParamsSchema;
  cacheSeconds?: number;
  handler: (context: {
    request: Request;
    actorId?: string;
    body: z.infer<BodySchema>;
    query: z.infer<QuerySchema>;
    params: z.infer<ParamsSchema>;
  }) => Promise<Response> | Response;
};

export function createRouteHandler<
  BodySchema extends z.ZodTypeAny = z.ZodAny,
  QuerySchema extends z.ZodTypeAny = z.ZodAny,
  ParamsSchema extends z.ZodTypeAny = z.ZodAny
>(config: RouteConfig<BodySchema, QuerySchema, ParamsSchema>) {
  return async (request: Request, params?: Record<string, string>): Promise<Response> => {
    const startTime = performance.now();
    const method = request.method;
    const url = new URL(request.url);
    const path = url.pathname;
    
    let actorId: string | undefined;

    try {
      // 1. Authentication
      if (config.requireAuth) {
        actorId = requireActor(request);
      }

      // 2. Rate Limiting
      if (config.rateLimit) {
        const repo = getApiContext().repository;
        if (config.rateLimit.type === "account") {
          if (!actorId) {
            throw new ApiError(401, "unauthorized", "Account rate limit requires authentication");
          }
          const accountLimit = await checkAccountLimit(repo, actorId);
          if (!accountLimit.allowed) {
            throw new ApiError(429, "too_many_requests", "Account limit exceeded", {
              retryAfterSeconds: accountLimit.retryAfterSeconds,
            });
          }
        } else if (config.rateLimit.type === "ip") {
          const ip =
            request.headers.get("cf-connecting-ip") ??
            request.headers.get("x-forwarded-for")?.split(",")[0]?.trim() ??
            "unknown";
          const ipLimit = await checkIpLimit(repo, ip);
          if (!ipLimit.allowed) {
            throw new ApiError(429, "too_many_requests", "IP limit exceeded", {
              retryAfterSeconds: ipLimit.retryAfterSeconds,
            });
          }
        }
      }

      // 3. Validation
      let parsedBody: any = undefined;
      let parsedQuery: any = undefined;
      let parsedParams: any = undefined;

      if (config.bodySchema) {
        parsedBody = await parseJsonBody(request, config.bodySchema);
      }

      if (config.querySchema) {
        const queryObj = Object.fromEntries(url.searchParams.entries());
        const result = config.querySchema.safeParse(queryObj);
        if (!result.success) {
          throw new ApiError(400, "bad_request", "Invalid query parameters");
        }
        parsedQuery = result.data;
      }

      if (config.paramsSchema) {
        const result = config.paramsSchema.safeParse(params || {});
        if (!result.success) {
          throw new ApiError(400, "bad_request", "Invalid route parameters");
        }
        parsedParams = result.data;
      }

      // 4. Execute Route
      let response = await config.handler({
        request,
        actorId,
        body: parsedBody,
        query: parsedQuery,
        params: parsedParams,
      });

      // 5. Caching
      if (config.cacheSeconds && response.status === 200) {
        // Need to create a new response to mutate headers if it's from a factory
        response = new Response(response.body, response);
        response.headers.set("Cache-Control", `public, max-age=${config.cacheSeconds}`);
      }

      // 6. Success Metrics & Logs
      const latency = performance.now() - startTime;
      metrics.recordHistogram("api_latency", latency, { method, path, status: String(response.status) });
      metrics.incrementCounter("api_requests_total", { method, path, status: String(response.status) });
      
      console.log(`[API SUCCESS] ${method} ${path} - ${response.status} (${latency.toFixed(2)}ms)`);
      
      return response;

    } catch (error: any) {
      // 7. Error Metrics & Logs
      const latency = performance.now() - startTime;
      const status = error instanceof ApiError ? error.status : 500;
      
      metrics.recordHistogram("api_latency", latency, { method, path, status: String(status) });
      metrics.incrementCounter("api_requests_total", { method, path, status: String(status) });
      metrics.incrementCounter("api_errors_total", { method, path, status: String(status) });
      
      console.error(`[API ERROR] ${method} ${path} - ${status} (${latency.toFixed(2)}ms)`, error);
      
      return apiFailure(request, error);
    }
  };
}
